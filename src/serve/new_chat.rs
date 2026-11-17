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
        Ok(found
            .filter(|_| !here && hub_main(Path::new(&allowed_cwd)))
            .map(|(project, _)| project.path.clone()))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "cwd check failed"))??;
    let own_worktree = main_of.is_some();
    let cwd = match main_of {
        Some(project) => chat_worktree(app, project).await?,
        None => body.cwd,
    };
    let mut call = CliCall {
        args: Vec::new(),
        cwd: cwd.clone().into(),
        session: None,
        adapter: "tmux-swarm".into(),
    };
    let create = app.chat_create.lock().await;
    run_cli(app, call.args(&["init"]), None).await?;
    let output = run_cli(app, call.args(&["session", "new", "lane"]), None).await?;
    let session = output.trim();
    if uuid::Uuid::parse_str(session).is_err() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "swarm returned an invalid session id",
        ));
    }
    call.session = Some(session.to_owned());
    drop(create);
    if own_worktree {
        let (root, path, id) = (app.root.clone(), cwd.clone(), session.to_owned());
        tokio::task::spawn_blocking(move || {
            swarm::task::record_chat(&root, Path::new(&path), Some(id))
        })
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "chat record task failed"))?
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err))?;
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
    Ok(
        json!({"ok": true, "session": session, "agent": "orchestrator", "pane": pane, "workspace": cwd}),
    )
}

/// True when `cwd` is the default-branch checkout of a worktree hub.
fn hub_main(cwd: &Path) -> bool {
    swarm::task::hub(cwd)
        .is_some_and(|hub| std::fs::canonicalize(cwd).is_ok_and(|cwd| cwd == hub.main))
}

/// A new worktree for one chat on `swarm/chat-<8 hex>`, made by the New workspace path, so it
/// starts from a fresh origin and runs the project's setup script. The first prompt names the
/// branch (ADR 0146). A failed setup starts no chat (ADR 0097).
async fn chat_worktree(app: &App, project: String) -> Result<String, ApiError> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let request = lifecycle::NewWorkspace {
        project,
        name: format!("chat-{}", &id[..8]),
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
        Some(state) => Err(error(
            StatusCode::BAD_GATEWAY,
            &format!(
                "The setup script of the new chat worktree {path} ended with {state}, so no chat started. {}",
                made["setup"]["error"].as_str().unwrap_or_default()
            ),
        )),
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
