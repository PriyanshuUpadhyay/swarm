//! Server commands.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandsQuery {
    pub(super) session: String,
    pub(super) agent: String,
}

/// How long a clean menu stays fresh, and how long a failed or partial read waits before the next.
pub(super) const COMMANDS_FRESH: Duration = Duration::from_secs(60);
pub(super) const COMMANDS_RETRY: Duration = Duration::from_secs(5);

/// The `/` menu rows of one agent (`swarm commands`). A cached list returns at once, and past its
/// age one probe refreshes it behind the reply, so a typed `/` waits on a CLI only the first time.
pub(super) async fn chat_commands(
    State(app): State<App>,
    Query(query): Query<CommandsQuery>,
) -> Result<Json<Value>, ApiError> {
    let (call, agent) = action_target(&app, &query.session, &query.agent)?;
    let slot = app
        .commands
        .lock()
        .unwrap()
        .entry((query.session, agent.clone()))
        .or_default()
        .clone();
    let cached = slot.last.lock().unwrap().clone();
    if let Some((stale_at, value)) = cached {
        if tokio::time::Instant::now() >= stale_at {
            tokio::spawn(async move {
                let Ok(_probe) = slot.probe.try_lock() else {
                    return;
                };
                if slot
                    .last
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|(at, _)| tokio::time::Instant::now() < *at)
                {
                    return;
                }
                probe_commands(&app, &slot, call, &agent).await;
            });
        }
        return Ok(Json(value));
    }
    let _probe = slot.probe.lock().await;
    if let Some((_, value)) = slot.last.lock().unwrap().clone() {
        return Ok(Json(value));
    }
    Ok(Json(probe_commands(&app, &slot, call, &agent).await))
}

/// Most paths the `@` menu gets, so a large checkout stays a small reply.
pub(super) const FILES_CAP: usize = 5000;

/// The `@` menu's paths: the files git tracks or would add under the chat's folder. The folder is
/// the session's own; the client names only the chat.
pub(super) async fn chat_files(
    State(app): State<App>,
    Query(query): Query<CommandsQuery>,
) -> Result<Json<Value>, ApiError> {
    let (call, _) = action_target(&app, &query.session, &query.agent)?;
    tokio::task::spawn_blocking(move || {
        let cwd = call.cwd.to_string_lossy().into_owned();
        let listed = lifecycle::git(
            &cwd,
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
        );
        let (files, note) = match listed {
            Ok(out) if out.status.success() => {
                let mut files: Vec<String> = out
                    .stdout
                    .split(|b| *b == 0)
                    .filter(|path| !path.is_empty())
                    .map(|path| String::from_utf8_lossy(path).into_owned())
                    .collect();
                files.sort();
                files.dedup();
                let note = (files.len() > FILES_CAP)
                    .then(|| format!("Showing the first {FILES_CAP} files of {}.", files.len()));
                files.truncate(FILES_CAP);
                (files, note)
            }
            _ => (
                Vec::new(),
                Some("Git cannot list the files of this chat's folder.".to_owned()),
            ),
        };
        Json(json!({ "files": files, "note": note }))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "file list task failed"))
}

/// Runs `swarm commands` and stores its answer. A failure is stored as a note, so the menu says
/// why, and a burst of `/` keys starts no second probe before `COMMANDS_RETRY`. A failed or partial
/// read keeps the rows of the last fuller list, so a slow CLI start does not empty the menu.
pub(super) async fn probe_commands(
    app: &Server,
    slot: &CommandsSlot,
    call: CliCall,
    agent: &str,
) -> Value {
    let read = run_cli(app, call.args(&["commands", agent, "--json"]), None)
        .await
        .and_then(|text| {
            serde_json::from_str::<Value>(&text)
                .map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid commands JSON from swarm"))
        });
    let (stale_at, mut value) = menu_entry(read, tokio::time::Instant::now());
    let mut last = slot.last.lock().unwrap();
    value = keep_fuller_rows(last.as_ref().map(|(_, value)| value), value);
    *last = Some((stale_at, value.clone()));
    value
}

/// A clean read stays fresh for `COMMANDS_FRESH`; a read with a note or an error is tried again
/// after `COMMANDS_RETRY`.
pub(super) fn menu_entry(
    read: Result<Value, ApiError>,
    now: tokio::time::Instant,
) -> (tokio::time::Instant, Value) {
    match read {
        Ok(value) if value["note"].is_null() => (now + COMMANDS_FRESH, value),
        Ok(value) => (now + COMMANDS_RETRY, value),
        Err(err) => (
            now + COMMANDS_RETRY,
            json!({"commands": [], "note": err.1.0["error"]}),
        ),
    }
}

/// A read with a note lists fewer rows than a full read; it keeps the earlier, longer rows.
pub(super) fn keep_fuller_rows(previous: Option<&Value>, mut value: Value) -> Value {
    let count = |value: &Value| value["commands"].as_array().map_or(0, Vec::len);
    if let Some(previous) = previous
        && !value["note"].is_null()
        && count(previous) > count(&value)
    {
        value["commands"] = previous["commands"].clone();
    }
    value
}

#[cfg(test)]
#[path = "commands/tests.rs"]
mod tests;
