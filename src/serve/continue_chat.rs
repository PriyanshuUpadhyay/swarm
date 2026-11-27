//! Server continue chat.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContinueChat {
    pub(super) old_session: String,
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) account: Option<String>,
    /// Absent keeps the profile's effort for that CLI.
    pub(super) effort: Option<String>,
    #[serde(default)]
    pub(super) carry: Carry,
}
/// What a switch keeps (ADR 0110). New values may appear.
#[derive(Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Carry {
    /// The same chat and CLI session, on the new run.
    Resume,
    /// Another CLI gets the whole history as a file.
    #[default]
    File,
    /// Another CLI gets an explicit summary.
    Summary,
    /// Another CLI gets no carried context.
    Clean,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CancelContinue {
    pub(super) old_session: String,
}
pub(super) struct HandoffRun {
    pub(super) phase: &'static str,
    pub(super) cancel: watch::Sender<bool>,
}
pub(super) fn register_continue(app: &App, body: &ContinueChat) -> Result<(), ApiError> {
    if swarm::providers::Provider::parse(&body.provider).is_none() {
        return Err(error(StatusCode::BAD_REQUEST, "unknown provider"));
    }
    validate_launch_options([
        Some(body.model.as_str()),
        body.account.as_deref(),
        body.effort.as_deref(),
    ])?;
    current_source(app, &body.old_session)?;
    let mut runs = app.handoffs.lock().unwrap();
    if runs.contains_key(&body.old_session) {
        return Err(error(
            StatusCode::CONFLICT,
            "A model switch is already running for this chat",
        ));
    }
    if runs.len() >= 64 {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Too many model switches",
        ));
    }
    let (cancel, _) = watch::channel(false);
    runs.insert(
        body.old_session.clone(),
        HandoffRun {
            phase: "preparing",
            cancel,
        },
    );
    Ok(())
}
pub(super) async fn continue_chat(
    State(app): State<App>,
    body: Result<Json<ContinueChat>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                e.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    register_continue(&app, &body)?;
    let old = body.old_session.clone();
    let result = enqueue_action(&app, Action::Continue(body)).await;
    // Execute owns cleanup after acceptance, even when the browser disconnects.
    if result
        .as_ref()
        .err()
        .is_some_and(|e| e.0 == StatusCode::SERVICE_UNAVAILABLE)
    {
        app.handoffs.lock().unwrap().remove(&old);
    }
    result.map(Json)
}
pub(super) async fn cancel_continue(
    State(app): State<App>,
    body: Result<Json<CancelContinue>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                e.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    let runs = app.handoffs.lock().unwrap();
    let run = runs
        .get(&body.old_session)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "no model switch for this chat"))?;
    if !matches!(run.phase, "preparing" | "summarizing") {
        return Err(error(
            StatusCode::CONFLICT,
            "The new agent is starting; wait for the switch to finish",
        ));
    }
    run.cancel.send_replace(true);
    Ok(Json(json!({"ok":true})))
}
pub(super) fn continue_phase(app: &App, old: &str, phase: &'static str) -> Result<(), ApiError> {
    let mut runs = app.handoffs.lock().unwrap();
    let run = runs
        .get_mut(old)
        .ok_or_else(|| error(StatusCode::CONFLICT, "model switch cancelled"))?;
    if *run.cancel.borrow() {
        return Err(error(StatusCode::CONFLICT, "model switch cancelled"));
    }
    run.phase = phase;
    app.hub.lock().unwrap().publish(Frame::new("handoff", json!({"old_session":old,"phase":phase,"canCancel":matches!(phase,"preparing"|"summarizing")}), None));
    Ok(())
}
pub(super) async fn handoff_rows(app: &App, old: &str) -> Result<Vec<Row>, ApiError> {
    let log = resolve_log(&app.root, old, "chair").map_err(|_| {
        error(
            StatusCode::CONFLICT,
            "The current chat has no readable transcript to carry forward",
        )
    })?;
    read_handoff_log(app, old, &log, None).await
}

async fn read_handoff_log(
    app: &App,
    old: &str,
    log: &Log,
    before: Option<u64>,
) -> Result<Vec<Row>, ApiError> {
    let end = row_offsets(&log.path, 1, before)
        .map_err(|e| error(StatusCode::CONFLICT, &e.to_string()))?
        .1;
    let state = read_row_window(app, &format!("{old}:chair"), log, 0, end).await?;
    Ok(view::rows(
        &state.records.values().cloned().collect::<Vec<_>>(),
        false,
        state.is_codex,
    ))
}

/// The chair's row of `swarm agents --json` for the chat `call` names, or null when it has none.
pub(super) async fn chair_agent(app: &App, call: &CliCall) -> Result<Value, ApiError> {
    let mut read = call.args(&["agents", "--json"]);
    // An imported chat has no adapter until its first pane. A chat on another adapter keeps it, so
    // the read lists that adapter's panes and not the app's own tmux server.
    read.default_adapter();
    let output = run_cli(app, read, None).await?;
    let agents: Value = serde_json::from_str(&output)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid agents JSON from swarm"))?;
    Ok(agents["agents"]
        .as_array()
        .and_then(|list| list.iter().find(|a| a["id"] == "orchestrator"))
        .cloned()
        .unwrap_or(Value::Null))
}

/// Same CLI: stop the chair and start its own session again on the new run (ADR 0110).
async fn resume_switch(app: &App, body: &ContinueChat) -> Result<Value, ApiError> {
    let old = &body.old_session;
    continue_phase(app, old, "preparing")?;
    let call = action_session(app, old)?;
    let chair = chair_agent(app, &call).await?;
    let (provider, chair_id): (Option<String>, Option<String>) = read_connection(&app.root)
        .and_then(|connection| {
            connection.query_row(
                "SELECT chair_provider, chair_id FROM session WHERE id = ?1",
                [old],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    if provider.as_deref() != Some(body.provider.as_str()) {
        return Err(error(
            StatusCode::CONFLICT,
            "Native resume needs the chat's own CLI.",
        ));
    }
    // A Codex or AGY chat learns its session id from its first log; before that a resume has
    // nothing to open, and the chair must keep running.
    if chair_id.is_none() {
        return Err(error(
            StatusCode::CONFLICT,
            "This chat has no session to resume before its first reply. Send a message first, then switch.",
        ));
    }
    // launch takes only the CLI's own levels; a refusal after the close would cost a restart.
    if let Some(effort) = body.effort.as_deref()
        && !swarm::providers::Provider::parse(&body.provider)
            .is_some_and(|cli| cli.info().efforts.contains(&effort))
    {
        return Err(error(
            StatusCode::CONFLICT,
            &format!("swarm: --effort {effort} is not a level of this CLI"),
        ));
    }
    let closed = chair["alive"] == true;
    if closed {
        require_idle(app, old, &chair).await?;
        continue_phase(app, old, "starting")?;
        run_cli(app, call.args(&["close", "--now", "orchestrator"]), None).await?;
    } else {
        continue_phase(app, old, "starting")?;
    }
    let account = resume_account(
        &body.provider,
        body.account.as_deref(),
        chair["account"].as_str(),
    );
    let mut call = call;
    // An imported chat has no adapter until its first pane.
    call.adapter = "tmux-swarm".into();
    let launch = |model: Option<&str>, effort: Option<&str>, account: Option<&str>| {
        run_cli(app, resume_launch(&call, model, effort, account), None)
    };
    if let Err(refused) = launch(Some(&body.model), body.effort.as_deref(), account).await {
        // A chair closed by this switch starts again on its old run, so the chat keeps a pane.
        // With no old model known, launch takes the role's runner, never the refused pick.
        if closed
            && let Err(again) = launch(
                chair["model"].as_str(),
                chair["effort"].as_str(),
                chair["account"].as_str(),
            )
            .await
        {
            let text = |(_, Json(body)): &ApiError| body["error"].as_str().unwrap_or("").to_owned();
            return Err(error(
                refused.0,
                &format!(
                    "{} The old run did not start again either: {}",
                    text(&refused),
                    text(&again)
                ),
            ));
        }
        return Err(refused);
    }
    Ok(json!({"ok": true, "session": old, "agent": "orchestrator"}))
}

/// The account a resume launches on. A Codex session lives in its account's home, so a switch that
/// names no account stays there instead of the launch default.
pub(super) fn resume_account<'a>(
    provider: &str,
    requested: Option<&'a str>,
    current: Option<&'a str>,
) -> Option<&'a str> {
    if provider == "codex" {
        requested.or(current)
    } else {
        requested
    }
}

pub(super) async fn execute_continue(app: &App, body: ContinueChat) -> Result<Value, ApiError> {
    execute_continue_timed(
        app,
        body,
        Duration::from_secs(90),
        Duration::from_secs(30),
        Duration::from_secs(1),
    )
    .await
}
pub(super) async fn execute_continue_timed(
    app: &App,
    body: ContinueChat,
    summary_wait: Duration,
    ready_wait: Duration,
    poll: Duration,
) -> Result<Value, ApiError> {
    let old = &body.old_session;
    let result = async {
        let mut cancelled = app.handoffs.lock().unwrap().get(old).ok_or_else(|| error(StatusCode::CONFLICT, "model switch cancelled"))?.cancel.subscribe();
        if *cancelled.borrow() { return Err(error(StatusCode::CONFLICT, "model switch cancelled")); }
        let mut call = current_source(app, old)?;

        call.default_adapter();
        if body.carry == Carry::Resume {
            let resumed = resume_switch(app, &body).await?;
            remember_started_model(app, old).await;
            return Ok(resumed);
        }
        let first = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(error(StatusCode::CONFLICT, "model switch cancelled")),
            prepared = prepare_carry(app, &body, &call, summary_wait, poll) => prepared?,
        };
        continue_phase(app, old, "starting")?;
        let started = new_chat_now(app, NewChat { cwd: call.cwd.to_string_lossy().into_owned(), role: None, provider: Some(body.provider.clone()), model: Some(body.model.clone()), account: body.account.clone(), effort: body.effort.clone(), prompt: None, place: Some(Place::Here) }).await?;
        let session = started["session"].as_str().unwrap();
        let next = CliCall { args: vec![], cwd: call.cwd.clone(), session: Some(session.into()), adapter: "tmux-swarm".into() };
        let done = async {
            if let Some(first) = first {
                continue_phase(app, old, "waiting")?;
                wait_for_carry(app, &next, session, &body.provider, ready_wait, poll).await?;
                continue_phase(app, old, "delivering")?;
                run_cli(app, next.args(&["type", "orchestrator"]), Some(first)).await?;
            }
            run_cli(app, call.args(&["session", "continue", session, old]), None).await?;
            remember_started_model(app, session).await;
            Ok(())
        }.await;
        if done.is_err() {
            discard_handoff(app, &next, session).await;
        }
        done.map(|()| started)
    }.await;
    let (phase, session, message) = match &result {
        Ok(value) => (
            "completed",
            value.get("session").cloned().unwrap_or(Value::Null),
            Value::Null,
        ),
        Err(e) => (
            if e.1.0["error"] == "model switch cancelled" {
                "cancelled"
            } else {
                "failed"
            },
            Value::Null,
            e.1.0["error"].clone(),
        ),
    };
    app.hub.lock().unwrap().publish(Frame::new("handoff", json!({"old_session":old,"phase":phase,"canCancel":false,"session":session,"error":message}), None));
    app.handoffs.lock().unwrap().remove(old);
    result
}

async fn prepare_carry(
    app: &App,
    body: &ContinueChat,
    call: &CliCall,
    summary_wait: Duration,
    poll: Duration,
) -> Result<Option<String>, ApiError> {
    let old = &body.old_session;
    continue_phase(app, old, "preparing")?;
    let chair = chair_agent(app, call).await?;
    let alive = chair["alive"] == true;

    if alive {
        require_idle(app, old, &chair).await?;
    }

    match body.carry {
        Carry::Clean => Ok(None),
        Carry::File => {
            let chain = read_connection(&app.root)
                .and_then(|db| swarm::store::continuation_logs(&db, old))
                .map_err(super::actions::history_read_error)?;
            let mut budget = ROWS_FIND_LIMIT as u64;
            let mut text = String::new();
            for row in chain {
                let log = match resolve_link_log(&row) {
                    Ok(log) => log,
                    Err(err) if err.0 == StatusCode::NOT_FOUND => continue,
                    Err(err) => return Err(err),
                };
                let size = std::fs::metadata(&log.path)
                    .map_err(|e| error(StatusCode::CONFLICT, &e.to_string()))?
                    .len();
                budget = budget.checked_sub(size).ok_or_else(file_carry_limit)?;
                let rows = read_handoff_log(app, &row.id, &log, Some(size)).await?;
                if !handoff::append_chat_text(&mut text, &rows, ROWS_FIND_LIMIT) {
                    return Err(file_carry_limit());
                }
            }
            if text.is_empty() {
                return Err(error(
                    StatusCode::CONFLICT,
                    "The current chat has no messages to carry forward",
                ));
            }
            let path = chat_file(app, old, text).await?;
            Ok(Some(handoff::file_message(&path.to_string_lossy())))
        }
        Carry::Summary => {
            if !alive {
                return Err(error(
                    StatusCode::CONFLICT,
                    "The old agent is not running to write a summary",
                ));
            }
            let baseline = handoff_rows(app, old).await?.len();
            continue_phase(app, old, "summarizing")?;
            let deadline = tokio::time::Instant::now() + summary_wait;
            let context = tokio::time::timeout_at(deadline, async {
                run_cli(
                    app,
                    call.args(&["type", "orchestrator"]),
                    Some(handoff::REQUEST.into()),
                )
                .await?;
                loop {
                    tokio::time::sleep(poll).await;
                    if let Ok(rows) = handoff_rows(app, old).await
                        && let Some(summary) = handoff::summary_turn(&rows, baseline)
                    {
                        return summary.ok_or_else(|| {
                            error(
                                StatusCode::CONFLICT,
                                "The old agent did not write a summary",
                            )
                        });
                    }
                }
            })
            .await
            .map_err(|_| {
                error(
                    StatusCode::GATEWAY_TIMEOUT,
                    "Timed out waiting for a chat summary",
                )
            })??;
            Ok(Some(handoff::summary_message(&context)))
        }
        Carry::Resume => unreachable!("native resume does not carry chat context"),
    }
}

fn file_carry_limit() -> ApiError {
    error(
        StatusCode::PAYLOAD_TOO_LARGE,
        "This chat is too large to carry as a file. Choose Summary or No history in Settings.",
    )
}

async fn require_idle(app: &App, old: &str, chair: &Value) -> Result<(), ApiError> {
    // A chat before its first turn has no log yet, and so no reply that runs.
    if chair["state"] == "working"
        || handoff_rows(app, old)
            .await
            .is_ok_and(|rows| view::is_active(&rows))
    {
        return Err(error(
            StatusCode::CONFLICT,
            "Wait for the current reply to finish, or stop it before switching model.",
        ));
    }
    Ok(())
}

async fn wait_for_carry(
    app: &App,
    next: &CliCall,
    session: &str,
    provider: &str,
    wait: Duration,
    poll: Duration,
) -> Result<(), ApiError> {
    tokio::time::timeout(wait, async {
        loop {
            tokio::time::sleep(poll).await;
            let agy = provider == "agy";
            let cmd = if agy { "agents" } else { "sessions" };

            let output = run_cli(app, next.args(&[cmd, "--json"]), None).await?;
            let listing: Value = serde_json::from_str(&output).map_err(|_| {
                error(
                    StatusCode::BAD_GATEWAY,
                    "invalid readiness listing from swarm",
                )
            })?;
            let ready = if agy {
                handoff::agy_ready(&listing)
            } else {
                listing[cmd].as_array().is_some_and(|rows| {
                    rows.iter()
                        .any(|s| s["id"] == session && handoff::ready(s, provider))
                })
            };
            if ready {
                return Ok(());
            }
        }
    })
    .await
    .map_err(|_| {
        error(
            StatusCode::GATEWAY_TIMEOUT,
            "The new agent did not become ready for the handoff",
        )
    })?
}

/// Saves the chat `text` of session `old` as `<swarm home>/handoffs/<old>.md`, mode 0600.
// ponytail: handoff files are never removed; prune by age if the folder grows.
async fn chat_file(app: &App, old: &str, text: String) -> Result<std::path::PathBuf, ApiError> {
    let dir = app.root.join("handoffs");
    let path = dir.join(format!("{old}.md"));
    let saved = path.clone();
    tokio::task::spawn_blocking(move || {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
        std::io::Write::write_all(
            &mut std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&saved)?,
            text.as_bytes(),
        )
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "chat file task failed"))?
    .map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Could not save the chat for the new agent: {e}"),
        )
    })?;
    Ok(path)
}

#[cfg(test)]
#[path = "continue_chat/tests.rs"]
mod tests;
