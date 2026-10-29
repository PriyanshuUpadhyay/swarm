//! Server action execute.

use super::*;
use std::collections::BTreeSet;

pub(super) fn action_target(
    app: &Server,
    session: &str,
    agent: &str,
) -> Result<(CliCall, String), ApiError> {
    let call = action_session(app, session)?;
    let agent = if agent == "chair" {
        "orchestrator"
    } else {
        agent
    };
    let connection = read_connection(&app.root)
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let exists = connection
        .query_row(
            "SELECT 1 FROM agent WHERE session_id = ?1 AND id = ?2",
            (session, agent),
            |_| Ok(()),
        )
        .optional()
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?
        .is_some();
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "unknown session or agent"));
    }
    if call.adapter.trim().is_empty() {
        return Err(error(
            StatusCode::CONFLICT,
            "This session does not record an adapter, so Swarm cannot reach its panes.",
        ));
    }
    Ok((call, agent.to_owned()))
}

pub(super) async fn execute_action(app: &App, action: Action) -> Result<Value, ApiError> {
    let default_call = || CliCall {
        args: Vec::new(),
        cwd: app.root.clone(),
        session: None,
        adapter: "tmux-swarm".into(),
    };
    let interrupt = matches!(action, Action::Interrupt(_));
    let archive = matches!(action, Action::Archive(_));
    let (call, text) = match action {
        Action::Send(body) => {
            if body.text.trim().is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "swarm: empty text"));
            }
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            let text = if action_provider(app, &body.session, &agent)?.as_deref() == Some("claude")
            {
                claude_text::cleaned(&body.text)
            } else {
                body.text
            };
            if text.trim().is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "swarm: empty text"));
            }
            (call.args(&["type", &agent]), Some(text))
        }
        Action::Interrupt(body) if interrupt_clears_box(app, &body)? => {
            return interrupt_claude(app, body).await;
        }
        Action::Interrupt(body) if interrupt_empties_pi(app, &body)? => {
            return interrupt_pi(app, body).await;
        }
        Action::Interrupt(body) | Action::Close(body) => {
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            if interrupt {
                (call.args(&["interrupt", &agent]), None)
            } else {
                // The app closes at once; only an orchestrator's `swarm close` waits for a wrap-up.
                (call.args(&["close", "--now", &agent]), None)
            }
        }
        Action::Key(body) => {
            if !matches!(body.key.as_str(), "Up" | "C-u") {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "only Up and C-u are allowed",
                ));
            }
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            (call.args(&["key", &agent, &body.key]), None)
        }
        Action::Answer(body) => {
            if body.prompt_id.trim().is_empty() || body.prompt_id.contains('\0') {
                return Err(error(StatusCode::BAD_REQUEST, "prompt_id required"));
            }
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            (
                call.args(&["answer", &agent, &body.prompt_id, &body.choice.to_string()]),
                None,
            )
        }
        Action::Archive(body) | Action::Unarchive(body) => {
            let verb = if archive { "archive" } else { "unarchive" };
            if body.ids.is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "session ids required"));
            }
            for id in &body.ids {
                action_session(app, id)?;
            }
            let mut call = default_call().args(&["session", verb]);
            call.args.extend(body.ids);
            (call, None)
        }
        Action::Choices(body) => {
            let app = app.clone();
            return tokio::task::spawn_blocking(move || apply_choice(&app, body))
                .await
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "choice task failed"))?;
        }
        Action::Lifecycle(action) => return execute_lifecycle(app, action).await,
        Action::UsageRefresh(body) => return refresh_usage(app, &body.provider).await,
        Action::Hud(body) if body.on => {
            let call = home_call(app, &["setup", "hud", "--yes"]);
            run_cli(app, call, None).await?;
            return hud_cli(app, "status").await;
        }
        Action::Hud(_) => return hud_cli(app, "uninstall").await,
        Action::Setup(body) => return run_setup_write(app, body).await,
        Action::Import(body) => return run_conductor_write(app, body).await,
        Action::Owner(write) => return run_owner_write(app, write).await,
        Action::RefreshView => {
            let app = app.clone();
            return tokio::task::spawn_blocking(move || refresh_tree(&app))
                .await
                .map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "tree refresh task failed",
                    )
                })?;
        }
        Action::PullBack(body) => return pull_back(app, body).await,
        Action::New(body) => return new_chat(app, body).await,
        Action::Resume(body) => return resume_chat(app, body).await,
        Action::Continue(body) => return execute_continue(app, body).await,
        Action::Archived => {
            let output = run_cli(
                app,
                default_call().args(&["sessions", "--json", "--archived"]),
                None,
            )
            .await?;
            let mut value: Value = serde_json::from_str(&output).map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "swarm returned invalid JSON",
                )
            })?;
            let sessions = value["sessions"].as_array_mut().ok_or_else(|| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "swarm returned invalid sessions",
                )
            })?;
            sessions.retain(|session| !session["archivedAt"].is_null());
            let app = app.clone();
            let titles = tokio::task::spawn_blocking(move || {
                let saved = choices_store::load(&app.root, false).map_err(choice_error)?;
                let mut snapshot = snapshot_value(&app)
                    .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
                app.tree_cache.lock().unwrap().discovery.augment(
                    &mut snapshot,
                    &saved.owner,
                    discovery::now(),
                );
                Ok::<_, ApiError>(archived_titles(&snapshot, &saved.owner))
            })
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "archive title task failed",
                )
            })??;
            for session in sessions {
                if let Some(title) = session["id"].as_str().and_then(|id| titles.get(id)) {
                    session["title"] = json!(title);
                }
            }
            value["ok"] = json!(true);
            return Ok(value);
        }
    };
    run_cli(app, call, text).await?;
    Ok(json!({"ok": true}))
}

fn archived_titles(snapshot: &Value, owner: &choices::OwnerChoices) -> BTreeMap<String, String> {
    let workspaces: BTreeSet<_> = snapshot["archived"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|session| session["workspace"].as_str())
        .collect();
    workspaces
        .into_iter()
        .flat_map(|workspace| tree::archived_chats(snapshot, workspace, owner))
        .map(|chat| (chat.id, chat.title))
        .collect()
}

#[cfg(test)]
#[path = "action_execute/tests.rs"]
mod tests;
