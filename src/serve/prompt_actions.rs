//! Server prompt actions.

use super::*;

pub(super) fn action_provider(
    app: &Server,
    session: &str,
    agent: &str,
) -> Result<Option<String>, ApiError> {
    let connection = read_connection(&app.root)
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let result = if agent == "orchestrator" || agent == "chair" {
        connection.query_row(
            "SELECT chair_provider FROM session WHERE id = ?1",
            [session],
            |row| row.get(0),
        )
    } else {
        connection.query_row(
            "SELECT provider FROM agent WHERE session_id = ?1 AND id = ?2",
            (session, agent),
            |row| row.get(0),
        )
    };
    result.map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))
}

pub(super) fn interrupt_empties_pi(app: &App, body: &ActionTarget) -> Result<bool, ApiError> {
    let (call, agent) = action_target(app, &body.session, &body.agent)?;
    let provider = action_provider(app, &body.session, &agent)?;
    Ok(queue::can_clear_pi_box(provider.as_deref(), &call.adapter))
}

pub(super) fn interrupt_clears_box(app: &App, body: &ActionTarget) -> Result<bool, ApiError> {
    let (call, agent) = action_target(app, &body.session, &body.agent)?;
    let provider = action_provider(app, &body.session, &agent)?;
    Ok(queue::can_pull_back(provider.as_deref(), &call.adapter))
}

/// Claude puts a stopped prompt back in its input box, and the next message the app types would
/// join it. So the box is cleared after the stop, with the presses that clear that prompt.
pub(super) async fn interrupt_claude(app: &App, body: ActionTarget) -> Result<Value, ApiError> {
    let (call, agent) = action_target(app, &body.session, &body.agent)?;
    let log_agent = if agent == "orchestrator" {
        "chair"
    } else {
        &agent
    };
    // A first turn can be stopped before its log is stored, and the stop must still run.
    let prompt = match resolve_log(&app.root, &body.session, log_agent) {
        Ok(log) => last_prompt(app, &log).await.unwrap_or_default(),
        Err(_) => String::new(),
    };
    run_cli(app, call.args(&["interrupt", &agent]), None).await?;
    // Claude puts the prompt back a moment after the stop; a press on an empty box does nothing.
    tokio::time::sleep(Duration::from_millis(300)).await;
    for index in 0..queue::clear_presses(&[prompt]) {
        if index > 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        run_cli(app, call.args(&["key", &agent, "C-u"]), None).await?;
    }
    Ok(json!({"ok": true}))
}

/// pi puts its queued messages back in its box on a stop. The app's Sent rows hold their exact
/// text, so the box is only emptied here, and the app moves the rows to the draft on "cleared".
pub(super) async fn interrupt_pi(app: &App, body: ActionTarget) -> Result<Value, ApiError> {
    let (call, agent) = action_target(app, &body.session, &body.agent)?;
    run_cli(app, call.args(&["interrupt", &agent]), None).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // The stop ran, so a box that cannot be read is not an error of the stop.
    let output = run_cli(app, call.args(&["clear-box", &agent]), None)
        .await
        .unwrap_or_default();
    let (state, held) = cleared_box(&output);
    Ok(json!({"ok": true, "box": state, "held": held}))
}

/// `swarm clear-box` prints "empty", or "cleared" and the text the box held, which the screen
/// joins with spaces. The app restores the Sent rows that this text holds.
fn cleared_box(output: &str) -> (&'static str, Option<&str>) {
    match output.split_once('\n') {
        Some(("cleared", held)) => ("cleared", Some(held.trim())),
        _ if output.trim() == "empty" => ("empty", None),
        _ => ("unknown", None),
    }
}

pub(super) async fn last_prompt(app: &App, log: &Log) -> Result<String, ApiError> {
    let (start, end) = row_offsets(&log.path, 200, None)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript read failed"))?;
    let state = read_row_window(&app.tool, log, start, end).await?;
    Ok(state
        .records
        .values()
        .filter(|record| record.event["type"] == "user_message_chunk")
        .filter_map(|record| record.event["text"].as_str())
        .next_back()
        .unwrap_or_default()
        .to_owned())
}

pub(super) async fn pull_back(app: &App, body: ActionTarget) -> Result<Value, ApiError> {
    let (call, agent) = action_target(app, &body.session, &body.agent)?;
    let provider = action_provider(app, &body.session, &agent)?;
    if queue::can_clear_pi_box(provider.as_deref(), &call.adapter) {
        // The text is the app's own Sent rows; the screen joins pi's lines with spaces.
        let output = run_cli(app, call.args(&["clear-box", &agent, "--dequeue"]), None).await?;
        let (state, held) = cleared_box(&output);
        let outcome = if state == "cleared" {
            "pulled"
        } else {
            "alreadySent"
        };
        return Ok(json!({"ok":true,"outcome":outcome,"text":null,"held":held}));
    }
    if !queue::can_pull_back(provider.as_deref(), &call.adapter) {
        return Err(error(
            StatusCode::CONFLICT,
            "Pull-back needs Claude or pi and a tmux adapter.",
        ));
    }
    let log_agent = if agent == "orchestrator" {
        "chair"
    } else {
        &agent
    };
    let log = resolve_log(&app.root, &body.session, log_agent)?;
    let (_, mut end) = row_offsets(&log.path, 1, None)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript read failed"))?;
    let state = read_row_window(&app.tool, &log, 0, end).await?;
    let pending = queue::replay(state.records.values().map(|record| &record.event));
    let Some(last) = pending.iter().rfind(|text| queue::is_owner(text)) else {
        return Ok(json!({"ok":true,"outcome":"alreadySent","text":null}));
    };
    if !pending.iter().all(|text| queue::is_owner(text)) {
        return Err(error(
            StatusCode::CONFLICT,
            "The agent has its own message in the queue. Edit after it is delivered.",
        ));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_millis(1500);
    run_cli(app, call.args(&["key", &agent, "Up"]), None).await?;
    let mut after = Vec::new();
    let decision = loop {
        let (_, next) = row_offsets(&log.path, 1, None)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript read failed"))?;
        if next < end {
            break queue::PullBack::Unconfirmed;
        }
        if next > end {
            match tokio::time::timeout_at(deadline, read_row_window(&app.tool, &log, end, next))
                .await
            {
                Ok(Ok(state)) => {
                    after.extend(state.records.into_values().map(|record| record.event));
                    end = next;
                }
                _ => break queue::PullBack::Unconfirmed,
            }
        }
        let decision =
            queue::PullBack::decide(&pending, &after, tokio::time::Instant::now() >= deadline);
        if decision != queue::PullBack::Waiting {
            break decision;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    for index in 0..decision.clear_presses(last) {
        if index > 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        run_cli(app, call.args(&["key", &agent, "C-u"]), None).await?;
    }
    let (outcome, text) = match decision {
        queue::PullBack::Pulled { owner, .. } => ("pulled", Some(owner.join("\n"))),
        queue::PullBack::AlreadySent { .. } => ("alreadySent", None),
        queue::PullBack::Unconfirmed | queue::PullBack::Waiting => ("unconfirmed", None),
    };
    Ok(json!({"ok":true,"outcome":outcome,"text":text}))
}

#[cfg(test)]
#[path = "prompt_actions/tests.rs"]
mod tests;
