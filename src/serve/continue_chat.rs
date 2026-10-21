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
/// What a switch keeps (ADR 0108). New values may appear.
#[derive(Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Carry {
    /// The same chat and CLI session, on the new run.
    Resume,
    /// A new chat that gets this chat's summary.
    #[default]
    Summary,
    /// A new chat with nothing from this one.
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
    if !swarm::config::valid_name(&body.model)
        || [&body.account, &body.effort]
            .into_iter()
            .flatten()
            .any(|value| !swarm::config::valid_name(value))
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

/// Same CLI: stop the chair and start its own session again on the new run (ADR 0108).
async fn resume_switch(app: &App, body: &ContinueChat) -> Result<Value, ApiError> {
    let old = &body.old_session;
    continue_phase(app, old, "preparing")?;
    let call = action_session(app, old)?;
    let output = run_cli(app, call.args(&["agents", "--json"]), None).await?;
    let agents: Value = serde_json::from_str(&output)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid agents JSON from swarm"))?;
    let chair = agents["agents"]
        .as_array()
        .and_then(|list| list.iter().find(|a| a["id"] == "orchestrator"))
        .cloned()
        .unwrap_or(Value::Null);
    if action_provider(app, old, "orchestrator")?.as_deref() != Some(body.provider.as_str()) {
        return Err(error(
            StatusCode::CONFLICT,
            "Only the chat's own CLI can keep this chat. Choose Summary or Start clean.",
        ));
    }
    // A Codex session lives in one account's home, so only that account resumes it.
    if body.provider == "codex"
        && body
            .account
            .as_deref()
            .is_some_and(|a| a != "auto" && Some(a) != chair["account"].as_str())
    {
        return Err(error(
            StatusCode::CONFLICT,
            "A Codex chat stays in its account. Choose Summary or Start clean to move it.",
        ));
    }
    if chair["alive"] == true {
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
        continue_phase(app, old, "starting")?;
        run_cli(app, call.args(&["close", "orchestrator"]), None).await?;
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
        let mut args = vec!["launch", "orchestrator", "chat", "--resume"];
        for (flag, value) in [
            ("--model", model),
            ("--effort", effort),
            ("--account", account),
        ] {
            if let Some(value) = value {
                args.extend([flag, value]);
            }
        }
        run_cli(app, call.args(&args), None)
    };
    if let Err(refused) = launch(Some(&body.model), body.effort.as_deref(), account).await {
        // The chair is closed by now, so its old run starts again and the chat keeps a pane.
        let _ = launch(
            chair["model"].as_str().or(Some(&body.model)),
            chair["effort"].as_str(),
            chair["account"].as_str(),
        )
        .await;
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
        if body.carry == Carry::Resume { return resume_switch(app, &body).await; }
        let prepare = async {
            continue_phase(app, old, "preparing")?;
            // Clean carries nothing, so it needs neither the log nor the chair.
            if body.carry == Carry::Clean {
                return Ok((action_session(app, old)?, String::new(), false));
            }
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
        let started = new_chat(app, NewChat { cwd: call.cwd.to_string_lossy().into_owned(), role: None, provider: Some(body.provider.clone()), model: Some(body.model.clone()), account: body.account.clone(), effort: body.effort.clone() }).await?;
        let session = started["session"].as_str().unwrap();
        if body.carry == Carry::Clean {
            run_cli(app, call.args(&["session", "continue", session, old]), None).await?;
            return Ok(started);
        }
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
