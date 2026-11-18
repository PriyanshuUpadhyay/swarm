//! A new chat's start in the background, step by step (ADR 0147).

use super::*;

/// How many `done` frames the hub keeps for pages that reconnect.
const STARTS_KEPT: usize = 64;

/// `GET /api/chat/start?id=`: `{done: frame}` for a recent start, `{running: true}` while it
/// runs, and `{}` for an id the server does not know or no longer keeps. A page whose stream
/// was down reads it, as the frame then never reached it.
pub(in crate::serve) async fn chat_start_done(
    State(app): State<App>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    let id = query.get("id").map(String::as_str).unwrap_or_default();
    let hub = app.hub.lock().unwrap();
    if let Some((_, frame)) = hub.starts_done.iter().find(|(start, _)| start == id) {
        return Json(json!({"done": frame}));
    }
    Json(if hub.starts_running.contains(id) {
        json!({"running": true})
    } else {
        json!({})
    })
}

/// The worktree a start makes or opens: the places of `Place` other than `here`.
#[derive(Clone)]
pub(super) enum Worktree {
    New,
    Branch { branch: String },
    Pr { number: u64 },
    Issue { number: u64 },
}

/// What a checked start launches, and where.
pub(super) struct Launch {
    pub(super) body: NewChat,
    pub(super) role: String,
    pub(super) account: Option<String>,
    pub(super) worktree: Option<(String, Worktree)>,
}

/// One chat start in the background, and the step it is on.
pub(super) struct Start {
    pub(super) app: App,
    pub(super) id: String,
    pub(super) step: &'static str,
}

impl Start {
    /// Publishes a `chat-start` frame for this start, with the words the page shows for its step.
    pub(super) fn frame(&self, step: &str, state: &str, extra: Value) {
        let words = match step {
            "worktree" => Some("Make the worktree"),
            "setup" => Some("Run the setup script"),
            "start" => Some("Start the agent"),
            "send" => Some("Send your prompt"),
            _ => None,
        };
        let mut payload = json!({"start": self.id, "step": step, "state": state, "words": words});
        if let Value::Object(extra) = extra {
            payload.as_object_mut().unwrap().extend(extra);
        }

        let mut hub = self.app.hub.lock().unwrap();
        if step == "done" {
            hub.starts_running.remove(&self.id);
            if hub.starts_done.len() == STARTS_KEPT {
                hub.starts_done.pop_front();
            }
            hub.starts_done
                .push_back((self.id.clone(), payload.clone()));
        }
        hub.publish(Frame::new("chat-start", payload, None));
    }

    /// Marks `step` running; the previous step completed.
    fn enter(&mut self, step: &'static str) {
        self.step = step;
        self.frame(step, "running", json!({}));
    }

    fn complete(&self, extra: Value) {
        self.frame(self.step, "completed", extra);
    }

    pub(super) async fn run(mut self, launch: Launch) {
        match self.steps(&launch).await {
            Ok((session, workspace)) => {
                self.frame(
                    "done",
                    "completed",
                    json!({"session": session, "workspace": workspace}),
                );
            }
            Err(err) => {
                let text = shells::api_text(&err);
                eprintln!("swarm: a new chat did not start: {text}");
                self.frame(self.step, "failed", json!({"error": text}));
                self.frame("done", "failed", json!({"error": text}));
            }
        }
    }

    /// Makes the worktree, the session and the chair, then sends the first prompt. Returns the
    /// session and its folder.
    pub(super) async fn steps(&mut self, launch: &Launch) -> Result<(String, String), ApiError> {
        let (body, role, account) = (&launch.body, &launch.role, launch.account.as_deref());
        let app = self.app.clone();
        let prompt = body
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty());

        let (cwd, own_branch, prompt) = match &launch.worktree {
            Some((project, place)) => {
                let made = self
                    .chat_worktree(project.clone(), &body.cwd, place.clone(), prompt)
                    .await?;
                (made.path, made.own_branch, made.prompt)
            }
            None => (body.cwd.clone(), None, prompt.map(str::to_owned)),
        };

        self.enter("start");
        // In a hub, a chat that did not start removes its own worktree, so a retry does not add
        // another; outside a hub the worktree stays (ADR 0147).
        let session = match open_session(&app, &cwd).await {
            Ok(session) => session,
            Err(err) => {
                if let Some(branch) = &own_branch {
                    forget_chat(&app, &cwd, None, branch).await;
                }
                return Err(err);
            }
        };

        let started = start_chair(
            &app,
            body,
            role,
            account,
            &cwd,
            &session,
            own_branch.is_some(),
        )
        .await;
        if let (Some(branch), Err(_)) = (&own_branch, &started) {
            forget_chat(&app, &cwd, Some(&session), branch).await;
        }
        let warning = started?;
        self.complete(json!({"session": session, "warning": warning}));

        // `swarm type` waits for the agent's input box, so the prompt goes once it is ready.
        if let Some(text) = prompt {
            self.enter("send");
            let send = SendAction {
                session: session.clone(),
                agent: "orchestrator".into(),
                text,
            };

            // The chat runs, so a failed send leaves the prompt to the owner and still opens it.
            match enqueue_action(&app, Action::Send(send)).await {
                Ok(_) => self.complete(json!({})),
                Err(err) => {
                    self.frame("send", "failed", json!({"error": shells::api_text(&err)}));
                }
            }
        }

        Ok((session, cwd))
    }

    /// A worktree for the chat in `project`, made by the New workspace path, so it starts from a
    /// fresh origin and runs the project's setup script. Place `new` makes `swarm/` and the
    /// prompt's name (ADR 0147), or `swarm/chat-<8 hex>` with no prompt, which the first prompt
    /// names later (ADR 0146). A failed setup starts no chat (ADR 0097); in a hub, a worktree of
    /// place `new` then goes when it holds no change. Any other worktree stays, as New workspace
    /// kept it.
    async fn chat_worktree(
        &mut self,
        project: String,
        cwd: &str,
        place: Worktree,
        prompt: Option<&str>,
    ) -> Result<Made, ApiError> {
        self.enter("worktree");
        // The hub is found from the checkout; a hub project's own folder is no checkout.
        let checkout = cwd.to_owned();

        // Held from the name pick to the new worktree; the setup script runs after it.
        let lock = self
            .app
            .worktree_locks
            .lock()
            .unwrap()
            .entry(project.clone())
            .or_default()
            .clone();
        let held = lock.lock().await;

        let named = match (&place, prompt) {
            (Worktree::New, Some(prompt)) => {
                let prompt = prompt.to_owned();
                let app = self.app.clone();
                tokio::task::spawn_blocking(move || {
                    let target = chat_target(&app, &checkout)?;
                    let project = lifecycle::project(&target.tree, &target.project)
                        .map_err(lifecycle_error)?;
                    Ok::<_, ApiError>(prompt_worktree(&target, project, &checkout, &prompt).0)
                })
                .await
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "chat name task failed"))??
            }
            _ => None,
        };

        let own = matches!(place, Worktree::New);
        let (mode, name, branch, pr, issue) = match place {
            Worktree::New => {
                let name = named
                    .map(|branch| branch.trim_start_matches(swarm::task::PREFIX).to_owned())
                    .unwrap_or_else(swarm::task::chat_placeholder);
                (lifecycle::Mode::NewBranch, name, None, None, None)
            }

            Worktree::Branch { branch } => {
                let name = branch.rsplit('/').next().unwrap_or(&branch).to_owned();
                (
                    lifecycle::Mode::ExistingBranch,
                    name,
                    Some(branch),
                    None,
                    None,
                )
            }

            Worktree::Pr { number } => (
                lifecycle::Mode::Pr,
                format!("pr-{number}"),
                None,
                Some(number),
                None,
            ),

            Worktree::Issue { number } => (
                lifecycle::Mode::Issue,
                String::new(),
                None,
                None,
                Some(number),
            ),
        };

        let prefix = own.then(|| swarm::task::PREFIX.to_owned());
        let request = lifecycle::NewWorkspace {
            project,
            name: name.clone(),
            folder: None,
            prefix: prefix.clone(),
            mode,
            base: None,
            branch,
            pr,
            issue,
        };

        let (made, setup) =
            apply_lifecycle_action(&self.app, LifecycleAction::New(request)).await?;
        drop(held);
        if let Some(existing) = made["existing"].as_object() {
            return Err(error(
                StatusCode::CONFLICT,
                &format!(
                    "The branch {} already exists. Pick it under Existing branch.",
                    existing["branch"].as_str().unwrap_or_default()
                ),
            ));
        }

        let path = made["workspace"]
            .as_str()
            .filter(|_| made["created"] == true || !own)
            .ok_or_else(|| {
                error(
                    StatusCode::CONFLICT,
                    "Swarm made no worktree for the chat; try again",
                )
            })?
            .to_owned();
        // Only a hub keeps a record of the chat's worktree, so only there does the chat own it:
        // the rename and the cleanup of a failed start need that record (ADR 0146).
        let in_hub = swarm::task::hub(Path::new(&path)).is_some();
        let own_branch = (own && in_hub).then(|| format!("{}{name}", swarm::task::PREFIX));

        // An issue's text goes before the prompt, as the New workspace dialog put it in the draft.
        let issue_text = made["issue"].as_object().map(|issue| {
            let title = issue["title"].as_str().unwrap_or_default();
            let body = issue["body"].as_str().unwrap_or_default().trim();
            let number = &issue["number"];
            if body.is_empty() {
                format!("Issue #{number}: {title}")
            } else {
                format!("Issue #{number}: {title}\n\n{body}")
            }
        });
        let prompt = match (issue_text, prompt) {
            (Some(issue), Some(prompt)) => Some(format!("{issue}\n\n{prompt}")),
            (issue, prompt) => issue.or(prompt.map(str::to_owned)),
        };

        let branch = lifecycle::check_git(&path, &["branch", "--show-current"]).ok();
        self.complete(json!({"workspace": path, "branch": branch}));

        if let Some(script) = setup {
            self.enter("setup");
            let result = run_setup(&self.app, &path, &script, SETUP_TIMEOUT).await;

            if result["state"] != "completed" {
                if let Some(branch) = &own_branch {
                    forget_chat(&self.app, &path, None, branch).await;
                }

                return Err(error(
                    StatusCode::BAD_GATEWAY,
                    &format!(
                        "The setup script of {path} ended with {}, so no chat started. {}",
                        result["state"].as_str().unwrap_or("an error"),
                        result["error"].as_str().unwrap_or_default()
                    ),
                ));
            }
            self.complete(json!({}));
        }

        Ok(Made {
            path,
            own_branch,
            prompt,
        })
    }
}

/// The folder a chat start made or opened, its branch when the chat owns the worktree, and the
/// first message to send.
struct Made {
    path: String,
    own_branch: Option<String>,
    prompt: Option<String>,
}
