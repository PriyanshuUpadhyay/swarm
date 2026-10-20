//! Server continue chat.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContinueChat {
    pub(super) old_session: String,
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) account: Option<String>,
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
    if !matches!(body.provider.as_str(), "claude" | "codex") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Model switching supports Claude and Codex chats",
        ));
    }
    if !swarm::config::valid_name(&body.model)
        || body
            .account
            .as_deref()
            .is_some_and(|a| !swarm::config::valid_name(a))
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid launch option"));
    }
    action_session(app, &body.old_session)?;
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
    let end = row_offsets(&log.path, 1, None)
        .map_err(|e| error(StatusCode::CONFLICT, &e.to_string()))?
        .1;
    let state = read_row_window(&app.tool, &log, 0, end).await?;
    Ok(view::rows(
        &state.records.values().cloned().collect::<Vec<_>>(),
        false,
        state.is_codex,
    ))
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
        let prepare = async {
            continue_phase(app, old, "preparing")?;
            let rows = handoff_rows(app, old).await?;
            let call = action_session(app, old)?;
            let output = run_cli(app, call.args(&["agents", "--json"]), None).await?;
            let agents: Value = serde_json::from_str(&output).map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid agents JSON from swarm"))?;
            let agents = agents["agents"].as_array().ok_or_else(|| error(StatusCode::BAD_GATEWAY, "missing agents list from swarm"))?;
            let alive = agents.iter().any(|a| a["id"] == "orchestrator" && a["alive"] == true);
            if alive && view::is_active(&rows) {
                return Err(error(StatusCode::CONFLICT, "Wait for the current reply to finish, or stop it before switching model."));
            }
            if alive {
                continue_phase(app, old, "summarizing")?;
                run_cli(app, call.args(&["type", "orchestrator"]), Some(handoff::REQUEST.into())).await?;
                let deadline = tokio::time::Instant::now() + summary_wait;
                loop {
                    let summary = tokio::time::timeout_at(deadline, async {
                        tokio::time::sleep(poll).await;
                        handoff_rows(app, old).await
                    }).await;
                    match summary {
                        Ok(Ok(new_rows)) => if let Some(context) = handoff::completed_summary(&new_rows, rows.len()) { return Ok((call, context, true)); },
                        Ok(Err(_)) => {}, // Swift keeps waiting if a summary poll cannot read the log.
                        Err(_) => break,
                    }
                }
            }
            let context = handoff::recent_context(&rows).ok_or_else(|| error(StatusCode::CONFLICT, "The current chat has no messages to carry forward"))?;
            Ok((call, context, false))
        };
        let (call, context, summary) = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(error(StatusCode::CONFLICT, "model switch cancelled")),
            prepared = prepare => prepared?,
        };
        continue_phase(app, old, "starting")?;
        let started = new_chat(app, NewChat { cwd: call.cwd.to_string_lossy().into_owned(), role: None, provider: Some(body.provider.clone()), model: Some(body.model), account: body.account }).await?;
        let session = started["session"].as_str().unwrap();
        continue_phase(app, old, "waiting")?;
        let deadline = tokio::time::Instant::now() + ready_wait;
        let ready = tokio::time::timeout_at(deadline, async {
            loop {
                tokio::time::sleep(poll).await;
                let output = run_cli(app, call.args(&["sessions", "--json"]), None).await?;
                let listing: Value = serde_json::from_str(&output).map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid sessions JSON from swarm"))?;
                let sessions = listing["sessions"].as_array().ok_or_else(|| error(StatusCode::BAD_GATEWAY, "missing sessions list from swarm"))?;
                if sessions.iter().any(|s| s["id"] == session && handoff::ready(s, &body.provider)) { return Ok::<_, ApiError>(()); }
            }
        }).await;
        ready.map_err(|_| error(StatusCode::GATEWAY_TIMEOUT, "The new agent did not become ready to receive the summary"))??;
        continue_phase(app, old, "delivering")?;
        let new_call = CliCall { args: vec![], cwd: call.cwd.clone(), session: Some(session.into()), adapter: "tmux-swarm".into() };
        run_cli(app, new_call.args(&["type", "orchestrator"]), Some(handoff::first_message(&context, summary))).await?;
        run_cli(app, call.args(&["session", "continue", session, old]), None).await?;
        Ok(started)
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
