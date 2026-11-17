//! Server new chat.

use super::*;

pub(super) async fn new_chat(app: &App, body: NewChat) -> Result<Value, ApiError> {
    let role = body.role.as_deref().unwrap_or("chat");
    let account = body.account.as_deref().or(if body.provider.is_none() {
        Some("auto")
    } else {
        None
    });
    for value in [
        Some(role),
        body.provider.as_deref(),
        body.model.as_deref(),
        body.effort.as_deref(),
        account,
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
    let snapshot_app = app.clone();
    let allowed_cwd = body.cwd.clone();
    let here = body.here;
    let main_of = tokio::task::spawn_blocking(move || {
        let (tree, saved) = fresh_lifecycle_tree(&snapshot_app)?;
        let found = tree.workspace(&allowed_cwd);
        let workspace = found.map(|(_, w)| w);
        if workspace.is_none() && !tree.projects.iter().any(|p| p.path == allowed_cwd) {
            return Err(error(
                StatusCode::FORBIDDEN,
                "cwd is outside the workspace tree",
            ));
        }
        lifecycle::checked_path(&snapshot_app.root, &allowed_cwd, true).map_err(lifecycle_error)?;
        if is_read_only(&tree, &saved, &allowed_cwd) {
            return Err(read_only_error("chat"));
        }
        // The tree is fresh, so its hub-main mark is the one the menu shows (ADR 0146).
        Ok(found
            .filter(|(_, workspace)| !here && workspace.hub_main)
            .map(|(project, _)| project.path.clone()))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "cwd check failed"))??;
    let own_worktree = main_of.is_some();
    let cwd = match main_of {
        Some(project) => chat_worktree(app, project).await?,
        None => body.cwd.clone(),
    };

    // A chat that did not start removes its own worktree, so a retry does not add another.
    let session = match open_session(app, &cwd).await {
        Ok(session) => session,
        Err(err) => {
            if own_worktree {
                forget_chat(app, &cwd, None).await;
            }
            return Err(err);
        }
    };
    let started = start_chair(app, &body, role, account, &cwd, &session, own_worktree).await;
    if own_worktree && started.is_err() {
        forget_chat(app, &cwd, Some(&session)).await;
    }
    started
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
async fn forget_chat(app: &App, cwd: &str, session: Option<&str>) {
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

    let (app, cwd) = (app.clone(), cwd.to_owned());
    let removed = tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&cwd);
        match swarm::task::remove_failed_chat(&app.root, &swarm::task::wt_program(), &path) {
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

/// Records a chat worktree, then launches the chair of `session`.
async fn start_chair(
    app: &App,
    body: &NewChat,
    role: &str,
    account: Option<&str>,
    cwd: &str,
    session: &str,
    own_worktree: bool,
) -> Result<Value, ApiError> {
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
    let pane = first_line(&output)
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "swarm returned no pane"))?;
    Ok(json!({
        "ok": true, "session": session, "agent": "orchestrator", "pane": pane,
        "workspace": cwd, "warning": warning,
    }))
}

/// A new worktree for one chat on `swarm/chat-<8 hex>`, made by the New workspace path, so it
/// starts from a fresh origin and runs the project's setup script. The first prompt names the
/// branch (ADR 0146). A failed setup starts no chat (ADR 0097), and the worktree then goes when
/// it holds no change.
async fn chat_worktree(app: &App, project: String) -> Result<String, ApiError> {
    let request = lifecycle::NewWorkspace {
        project,
        name: swarm::task::chat_placeholder(),
        folder: None,
        prefix: Some(swarm::task::PREFIX.into()),
        mode: lifecycle::Mode::NewBranch,
        base: None,
        branch: None,
        pr: None,
        issue: None,
    };

    let made = execute_lifecycle(app, LifecycleAction::New(request)).await?;
    let path = made["workspace"]
        .as_str()
        .filter(|_| made["created"] == true)
        .ok_or_else(|| {
            error(
                StatusCode::CONFLICT,
                "Swarm made no worktree for the chat; try again",
            )
        })?;

    match made["setup"]["state"].as_str() {
        None | Some("completed") => Ok(path.to_owned()),
        Some(state) => {
            forget_chat(app, path, None).await;
            Err(error(
                StatusCode::BAD_GATEWAY,
                &format!(
                    "The setup script of the new chat worktree {path} ended with {state}, so no chat started. {}",
                    made["setup"]["error"].as_str().unwrap_or_default()
                ),
            ))
        }
    }
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
    let (tree, saved) = fresh_lifecycle_tree(app)?;
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

#[cfg(test)]
#[path = "new_chat/tests.rs"]
mod tests;
