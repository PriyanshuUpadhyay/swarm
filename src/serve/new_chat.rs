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
    tokio::task::spawn_blocking(move || {
        let (tree, saved) = fresh_lifecycle_tree(&snapshot_app)?;
        let workspace = tree.workspace(&allowed_cwd).map(|(_, w)| w);
        if workspace.is_none() && !tree.projects.iter().any(|p| p.path == allowed_cwd) {
            return Err(error(
                StatusCode::FORBIDDEN,
                "cwd is outside the workspace tree",
            ));
        }
        lifecycle::checked_path(&snapshot_app.root, &allowed_cwd, true).map_err(lifecycle_error)?;
        if workspace.is_some_and(|w| w.missing || w.removed)
            || saved.owner.archived.contains(&allowed_cwd)
        {
            return Err(error(
                StatusCode::CONFLICT,
                "This workspace is read-only. Restore it to start a chat.",
            ));
        }
        Ok(())
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "cwd check failed"))??;
    let mut call = CliCall {
        args: Vec::new(),
        cwd: body.cwd.into(),
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
    Ok(json!({"ok": true, "session": session, "agent": "orchestrator", "pane": pane}))
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
