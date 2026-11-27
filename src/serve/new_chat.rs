//! Server new chat.

use super::*;

/// `POST /api/chat/new`: checks the request, answers at once with a start id, and starts the
/// chat in the background (ADR 0149). Each step goes out as a `chat-start` frame with that id;
/// the last one, step `done`, carries the session and its folder, or the error.
pub(super) async fn new_chat(app: &App, body: NewChat) -> Result<Value, ApiError> {
    let (start, launch) = checked_start(app, body).await?;
    let id = start.id.clone();
    app.hub.lock().unwrap().begin_start(id.clone());

    // A panic in a step drops its task, so the outer task sends the `done` that it never sent.
    let watch = Start {
        app: app.clone(),
        id: id.clone(),
        step: Step::Start,
    };
    let running = tokio::spawn(start.run(launch));
    tokio::spawn(async move {
        if running.await.is_err() {
            let text = "The start stopped on an internal error. Look in the server log.";
            watch.frame(Step::Done, "failed", json!({"error": text}));
        }
    });
    Ok(json!({"ok": true, "start": id}))
}

/// A new chat that a model switch starts in `body.cwd`; it waits for the chat to start.
pub(super) async fn new_chat_now(app: &App, body: NewChat) -> Result<Value, ApiError> {
    let (mut start, launch) = checked_start(app, body).await?;
    let (session, workspace, _) = start.steps(&launch).await?;
    Ok(json!({"ok": true, "session": session, "workspace": workspace}))
}

/// Checks a new chat request and the folder it names, before any change.
async fn checked_start(app: &App, body: NewChat) -> Result<(Start, Launch), ApiError> {
    let role = body.role.clone().unwrap_or_else(|| "chat".into());
    let account = body.account.clone().or(if body.provider.is_none() {
        Some("auto".into())
    } else {
        None
    });

    for value in [
        Some(role.as_str()),
        body.provider.as_deref(),
        body.model.as_deref(),
        body.effort.as_deref(),
        account.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !swarm::config::valid_name(value) {
            return Err(error(StatusCode::BAD_REQUEST, "invalid launch option"));
        }
    }

    if body.cwd.contains('\0') || !Path::new(&body.cwd).is_absolute() {
        return Err(error(StatusCode::BAD_REQUEST, "absolute cwd required"));
    }

    let place = body.place.clone();
    let snapshot_app = app.clone();
    let allowed_cwd = body.cwd.clone();

    let project_of = tokio::task::spawn_blocking(move || {
        let target = chat_target(&snapshot_app, &allowed_cwd, true)?;
        if let Some(common) = &target.worktree_only {
            match &place {
                None | Some(Place::Here) => return Err(read_only_error("chat")),
                Some(Place::Branch { branch }) => {
                    let name = branch.strip_prefix("origin/").unwrap_or(branch);
                    if lifecycle::references(common)
                        .map_err(lifecycle_error)?
                        .checkouts
                        .contains_key(name)
                    {
                        return Err(error(
                            StatusCode::CONFLICT,
                            "Pick a branch with no worktree.",
                        ));
                    }
                }
                _ => {}
            }
        }

        // With no place, a hub's main makes the chat's own worktree and any other folder starts
        // the chat in itself (ADR 0146); the composer's `new` makes one in any git project.
        Ok::<_, ApiError>(match place {
            Some(Place::Here) => None,
            None => target.hub_main.then_some((target.project, Worktree::New)),
            Some(Place::New) => Some((target.project, Worktree::New)),
            Some(Place::Branch { branch }) => Some((target.project, Worktree::Branch { branch })),
            Some(Place::Pr { number }) => Some((target.project, Worktree::Pr { number })),
            Some(Place::Issue { number }) => Some((target.project, Worktree::Issue { number })),
        })
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "cwd check failed"))??;

    let start = Start {
        app: app.clone(),
        id: uuid::Uuid::new_v4().to_string(),
        step: Step::Start,
    };
    let launch = Launch {
        body,
        role,
        account,
        worktree: project_of,
    };
    Ok((start, launch))
}

/// The project a new chat in `cwd` belongs to, and whether `cwd` is its hub's main checkout.
struct Target {
    project: String,
    hub_main: bool,
    worktree_only: Option<String>,
    tree: tree::Tree,
    saved: choices_store::Saved,
}

/// Checks the folder named by a new chat. An archived Git root can name a project for a new
/// worktree, but cannot be the chat's folder. A start passes `recheck_projects`, so its hub-main mark is
/// that of the git folders now (ADR 0146); a menu or a preview reads the catalog of the last 10 s.
fn chat_target(app: &App, cwd: &str, recheck_projects: bool) -> Result<Target, ApiError> {
    let (tree, saved) = fresh_lifecycle_tree(app, recheck_projects)?;
    let found = tree.workspace(cwd);
    let project = found
        .map(|(project, _)| project)
        .or_else(|| tree.projects.iter().find(|p| p.path == cwd))
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "cwd is outside the workspace tree"))?;

    lifecycle::checked_path(&app.root, cwd, true).map_err(lifecycle_error)?;
    let worktree_only = if is_read_only(&tree, &saved, cwd) {
        if project.path != cwd || !found.is_some_and(|(_, w)| !w.missing && !w.removed) {
            return Err(read_only_error("chat"));
        }
        let common = lifecycle::common(project).ok_or_else(|| read_only_error("chat"))?;
        lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
        Some(common.to_owned())
    } else {
        None
    };

    let hub_main = found.is_some_and(|(_, workspace)| workspace.hub_main);
    Ok(Target {
        project: project.path.clone(),
        hub_main,
        worktree_only,
        tree,
        saved,
    })
}

impl Target {
    /// The tree's entry for the target's project.
    fn tree_project(&self) -> Result<&tree::Project, ApiError> {
        lifecycle::project(&self.tree, &self.project).map_err(lifecycle_error)
    }
}

/// The branch and folder that a new worktree named from `prompt` gets: the first name whose
/// branch and folder are both free, with the folder create_workspace makes for it.
fn prompt_worktree(
    root: &Path,
    target: &Target,
    project: &tree::Project,
    cwd: &str,
    prompt: &str,
) -> (Option<String>, Option<PathBuf>) {
    let (setting, _, _) = lifecycle::defaults(project, &target.saved.owner);
    let parent = lifecycle::worktree_parent(root, project, &setting).ok();

    let folder_of = |branch: &str| {
        let name = swarm::task::task_name(branch);
        parent
            .as_ref()
            .map(|parent| parent.join(lifecycle::slug(name)))
    };

    let branch = swarm::task::chat_preview(Path::new(cwd), prompt, |branch| {
        folder_of(branch).is_none_or(|folder| !folder.exists())
    });
    let folder = branch.as_deref().and_then(folder_of);
    (branch, folder)
}

fn chat_call(cwd: &str, session: Option<&str>) -> CliCall {
    CliCall {
        args: Vec::new(),
        cwd: cwd.into(),
        session: session.map(str::to_owned),
        adapter: "tmux-swarm".into(),
    }
}

/// Makes the swarm session of a new chat in `cwd`, and returns its id.
async fn open_session(app: &App, cwd: &str) -> Result<String, ApiError> {
    let call = chat_call(cwd, None);
    let _create = app.chat_create.lock().await;
    let root = app.root.clone();
    let cwd = cwd.to_owned();
    // Existing-branch starts may resolve to a different folder than the request named.
    tokio::task::spawn_blocking(move || {
        let saved = choices_store::load(&root, true).map_err(choice_error)?;
        if saved.owner.archived.contains(&cwd) {
            return Err(read_only_error("chat"));
        }
        Ok(())
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "cwd check failed"))??;
    run_cli(app, call.args(&["init"]), None).await?;
    let output = run_cli(app, call.args(&["session", "new", "lane"]), None).await?;
    let session = output.trim();
    if uuid::Uuid::parse_str(session).is_err() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "swarm returned an invalid session id",
        ));
    }
    Ok(session.to_owned())
}

/// Undoes a chat start that failed after its worktree was made. The session is archived, and
/// the worktree and its record go when it holds no change. A chair that may run keeps both,
/// because a launch can time out after its pane started: only a session with no chair row, or
/// one whose chair is known to be stopped, is undone.
async fn forget_chat(app: &App, cwd: &str, session: Option<&str>, branch: &str) {
    if let Some(session) = session {
        let call = chat_call(cwd, Some(session));
        let stopped = match chair_agent(app, &call).await {
            Ok(chair) => chair.is_null() || chair["alive"] == false,
            Err(_) => false,
        };
        if !stopped {
            eprintln!("swarm: kept the chat worktree {cwd}, because its chair may run");
            return;
        }

        // A session that stays must keep its folder.
        if let Err(err) = run_cli(app, call.args(&["session", "archive", session]), None).await {
            eprintln!(
                "swarm: kept the chat worktree {cwd}, because its session {session} stays: {}",
                shells::api_text(&err)
            );
            return;
        }
    }

    let (app, cwd, branch) = (app.clone(), cwd.to_owned(), branch.to_owned());
    let removed = tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&cwd);
        let wt = swarm::task::wt_program();
        match swarm::task::remove_failed_chat(&app.root, &wt, &path, &branch) {
            Ok(()) => {
                shells::close_all(&app, &cwd);
                action_execute::forget_folder(&app.root, &cwd);
            }
            Err(err) => {
                eprintln!("swarm: kept the chat worktree {cwd} of a chat that did not start: {err}")
            }
        }
    })
    .await;

    if let Err(err) = removed {
        eprintln!("swarm: the cleanup of a chat that did not start failed: {err}");
    }
}

/// Records a chat worktree, then launches the chair of `session`. Returns the record's warning.
async fn start_chair(
    app: &App,
    body: &NewChat,
    role: &str,
    account: Option<&str>,
    cwd: &str,
    session: &str,
    own_worktree: bool,
) -> Result<Option<String>, ApiError> {
    let mut call = chat_call(cwd, Some(session));

    let mut warning = None;
    if own_worktree {
        let (root, path, id) = (app.root.clone(), cwd.to_owned(), session.to_owned());
        let (_, found) = tokio::task::spawn_blocking(move || {
            swarm::task::record_chat(&root, Path::new(&path), Some(id))
        })
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "chat record task failed"))?
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err))?;
        if let Some(found) = &found {
            eprintln!("swarm: chat worktree {cwd}: {found}");
        }
        warning = found;
    }

    call.args = vec!["launch".into(), "orchestrator".into(), role.to_owned()];
    for (flag, value) in [
        ("--provider", body.provider.as_deref()),
        ("--model", body.model.as_deref()),
        ("--effort", body.effort.as_deref()),
        ("--account", account),
    ] {
        if let Some(value) = value {
            call.args.extend([flag.to_owned(), value.to_owned()]);
        }
    }
    let output = run_cli(app, call, None).await?;
    first_line(&output)
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "swarm returned no pane"))?;
    Ok(warning)
}

/// True when the workspace that holds `cwd` is archived, missing, or removed. That is the deepest
/// workspace of the tree whose folder contains `cwd`, as the sidebar files a chat; a chat in a
/// subfolder of an archived workspace is read-only too.
fn is_read_only(tree: &tree::Tree, saved: &choices_store::Saved, cwd: &str) -> bool {
    match tree.workspace_containing(cwd) {
        Some((_, w)) => w.missing || w.removed || saved.owner.archived.contains(&w.path),
        None => saved.owner.archived.contains(cwd),
    }
}

fn read_only_error(what: &str) -> ApiError {
    error(
        StatusCode::CONFLICT,
        &format!("This workspace is read-only. Restore it to start a {what}."),
    )
}

/// Refuses a new `what` (a chat, a shell) in an archived, missing, or removed workspace.
pub(super) fn refuse_read_only_workspace(app: &App, cwd: &str, what: &str) -> Result<(), ApiError> {
    let (tree, saved) = fresh_lifecycle_tree(app, true)?;
    if is_read_only(&tree, &saved, cwd) {
        return Err(read_only_error(what));
    }
    Ok(())
}

/// The first non-empty line of a CLI's output; `launch` prints the pane id there.
pub(super) fn first_line(output: &str) -> Option<&str> {
    output.lines().map(str::trim).find(|line| !line.is_empty())
}

/// Start the chat's own CLI session again in the chat, with its whole context (ADR 0107).
pub(super) async fn resume_chat(app: &App, body: ResumeChat) -> Result<Value, ApiError> {
    let mut call = action_session(app, &body.session)?;
    // An imported chat has no adapter until its first pane.
    call.adapter = "tmux-swarm".into();
    call.args = ["launch", "orchestrator", "chat", "--resume"]
        .map(String::from)
        .to_vec();
    let output = run_cli(app, call, None).await?;
    let pane = first_line(&output)
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "swarm returned no pane"))?;
    Ok(json!({"ok": true, "session": body.session, "agent": "orchestrator", "pane": pane}))
}

#[path = "new_chat/preview.rs"]
mod preview;
pub(super) use preview::new_chat_preview;
#[path = "new_chat/menu.rs"]
mod menu;
pub(super) use menu::{new_chat_commands, new_chat_files};
#[path = "new_chat/start.rs"]
pub(super) mod start;
use start::{Launch, Start, Step, Worktree};

#[cfg(test)]
#[path = "new_chat/tests.rs"]
mod tests;
