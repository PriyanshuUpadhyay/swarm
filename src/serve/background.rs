//! A chat's background tasks: the shells and subagents its live Claude agents started (ADR 0104).
//!
//! Claude Code runs them inside its own process, so nothing outside can stop them or type into
//! them. Its log names each one: a tool result with `toolUseResult.backgroundTaskId` (a shell) or
//! `toolUseResult.isAsync` (a subagent), and later a `<task-notification>` with the end state.

use super::*;

/// A task title keeps its first line, cut to this many characters.
const TITLE_CHARS: usize = 200;
/// How every Claude task output file name ends.
pub(super) const OUTPUT_SUFFIX: &str = ".output";

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum TaskKind {
    Shell,
    Agent,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub(super) struct Task {
    pub(super) id: String,
    pub(super) agent: String,
    pub(super) kind: TaskKind,
    pub(super) title: String,
    /// The notification's status (`completed`, `failed`, `killed`, ...), `running`, or `unknown`.
    pub(super) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) summary: Option<String>,
    #[serde(skip)]
    pub(super) output: Option<PathBuf>,
}

/// Every background task one live agent's Claude log names, oldest first. A task with no end
/// record shows `running` only while its output file is still there, since a restart or a cleanup
/// can drop the notification.
pub(super) fn tasks(agent: &str, log: &str) -> Vec<Task> {
    let mut titles: HashMap<String, String> = HashMap::new();
    let mut found: Vec<Task> = Vec::new();
    let mut ends: HashMap<String, (String, Option<String>)> = HashMap::new();
    for line in log.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if line.contains("<task-notification>")
            && let Some(text) = notice(&record)
            && let Some(id) = view::tagged(text, "task-id").map(str::trim)
        {
            let status = view::tagged(text, "status").unwrap_or("").trim();
            let summary = view::tagged(text, "summary").map(|s| s.trim().to_string());
            if !status.is_empty() {
                ends.insert(id.into(), (status.into(), summary));
            }
        }
        let content = record.pointer("/message/content").and_then(Value::as_array);
        if record["type"] == "assistant" {
            for block in content.into_iter().flatten() {
                if block["type"] == "tool_use"
                    && let Some(id) = block["id"].as_str()
                {
                    let input = &block["input"];
                    let title = input["description"]
                        .as_str()
                        .or_else(|| input["command"].as_str())
                        .unwrap_or("");
                    titles.insert(id.into(), title.into());
                }
            }
            continue;
        }
        let result = &record["toolUseResult"];
        let (id, kind) = if let Some(id) = result["backgroundTaskId"].as_str() {
            (id, TaskKind::Shell)
        } else if result["isAsync"] == true
            && let Some(id) = result["agentId"].as_str()
        {
            (id, TaskKind::Agent)
        } else {
            continue;
        };
        // The panel and the ticket name a shell by its id, so a task with none cannot be shown.
        if id.is_empty() {
            continue;
        }
        let block = content.and_then(|blocks| blocks.first());
        let call = block.and_then(|b| b["tool_use_id"].as_str()).unwrap_or("");
        let mut texts = Vec::new();
        if let Some(block) = block {
            strings(&block["content"], &mut texts);
        }
        let title = result["description"]
            .as_str()
            .map(str::to_string)
            .or_else(|| titles.get(call).cloned())
            .unwrap_or_default();
        found.retain(|task| task.id != id);
        found.push(Task {
            id: id.into(),
            agent: agent.into(),
            kind,
            title: title
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(TITLE_CHARS)
                .collect(),
            status: String::new(),
            summary: None,
            output: texts.iter().find_map(|text| output_path(text)),
        });
    }
    for task in &mut found {
        (task.status, task.summary) = match ends.remove(&task.id) {
            Some(end) => end,
            None if task.output.as_ref().is_some_and(|path| path.is_file()) => {
                ("running".into(), None)
            }
            None => ("unknown".into(), None),
        };
    }
    found
}

/// The text of a record that Claude Code writes for a task notification: the queued notice, the
/// prompt it becomes, or its attachment. A tool result or a prompt snapshot that quotes the tag is
/// not one.
fn notice(record: &Value) -> Option<&str> {
    let text = match record["type"].as_str()? {
        "queue-operation" => record["content"].as_str(),
        "user" => record.pointer("/message/content")?.as_str(),
        "attachment" if record["attachment"]["type"] == "queued_command" => {
            record.pointer("/attachment/prompt")?.as_str()
        }
        _ => None,
    }?;
    text.trim_start()
        .starts_with("<task-notification>")
        .then_some(text)
}

/// The output file a launch result names: `Output is being written to: <path>.output.` for a
/// shell, `output_file: <path>` for a subagent.
fn output_path(text: &str) -> Option<PathBuf> {
    let start = ["Output is being written to: ", "output_file: "]
        .iter()
        .find_map(|label| text.find(label).map(|at| at + label.len()))?;
    let rest = &text[start..];
    let end = rest.find(OUTPUT_SUFFIX)? + OUTPUT_SUFFIX.len();
    Some(PathBuf::from(&rest[..end]))
}

fn strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[derive(Deserialize)]
pub(super) struct BackgroundQuery {
    pub(super) session: String,
}

/// The live agents in a session's hub list.
pub(super) fn live_agents(agents: &Value) -> impl Iterator<Item = &Value> {
    agents
        .as_array()
        .into_iter()
        .flatten()
        .filter(|agent| agent["alive"] == true)
}

/// The live Claude agents of a session, as (agent id, log path).
fn claude_logs(app: &Server, session: &str) -> Vec<(String, PathBuf)> {
    let hub = app.hub.lock().unwrap();
    live_agents(&hub.agents[session])
        .filter(|agent| agent["provider"] == "claude")
        .filter_map(|agent| {
            let log = agent["log"].as_str().filter(|log| !log.is_empty())?;
            Some((agent["id"].as_str()?.to_string(), PathBuf::from(log)))
        })
        .collect()
}

/// Every background task of the session's live Claude agents.
pub(super) fn session_tasks(app: &Server, session: &str) -> Vec<Task> {
    claude_logs(app, session)
        .into_iter()
        .flat_map(|(agent, log)| {
            // ponytail: reads each whole log per call; keep an offset per log if a long chat lags.
            let text = std::fs::read_to_string(log).unwrap_or_default();
            tasks(&agent, &text)
        })
        .collect()
}

pub(super) async fn background(
    State(app): State<App>,
    Query(query): Query<BackgroundQuery>,
) -> Result<Json<Value>, ApiError> {
    let tasks = tokio::task::spawn_blocking(move || {
        let tasks = session_tasks(&app, &query.session);
        tasks
            .iter()
            .map(|task| {
                let mut value = json!(task);
                value["output"] = json!(task.kind == TaskKind::Shell && task.output.is_some());
                value
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "background read failed"))?;
    Ok(Json(json!({"tasks": tasks})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(value: Value) -> String {
        format!("{value}\n")
    }

    fn launch(call: &str, result: Value, text: &str) -> String {
        line(json!({
            "type": "user",
            "message": {"role": "user", "content": [{"tool_use_id": call, "type": "tool_result", "content": text}]},
            "toolUseResult": result,
        }))
    }

    fn call(id: &str, name: &str, input: Value) -> String {
        line(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": id, "name": name, "input": input}]},
        }))
    }

    fn notification(id: &str, status: &str) -> String {
        line(json!({
            "type": "queue-operation",
            "content": format!("<task-notification>\n<task-id>{id}</task-id>\n<status>{status}</status>\n<summary>Background command \"x\" {status}</summary>\n</task-notification>"),
        }))
    }

    #[test]
    fn a_log_names_shells_and_subagents_with_their_end_state() {
        let dir = std::env::temp_dir().join(format!("swarm-bg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let live_file = dir.join("b2.output");
        std::fs::write(&live_file, "still going\n").unwrap();
        let log = [
            call("t1", "Bash", json!({"command": "sleep 30", "description": "Sleep for 30 seconds", "run_in_background": true})),
            launch("t1", json!({"backgroundTaskId": "b1"}), "Command running in background with ID: b1. Output is being written to: /tmp/claude-1/x/s/tasks/b1.output. You will be notified when it completes."),
            // Moved to the background by Ctrl+B or a timeout: no run_in_background in the input.
            call("t2", "Bash", json!({"command": "cargo test"})),
            launch("t2", json!({"backgroundTaskId": "b2"}), &format!("Command running in background with ID: b2. Output is being written to: {}. You will be notified.", live_file.display())),
            call("t3", "Agent", json!({"description": "ignored", "run_in_background": true})),
            launch("t3", json!({"isAsync": true, "agentId": "a3", "description": "Research speakers"}), "Async agent launched successfully.\nagentId: a3\noutput_file: /tmp/claude-1/x/s/tasks/a3.output\nDo NOT Read"),
            "not json\n".into(),
            launch("t0", json!({"backgroundTaskId": ""}), "Output is being written to: /tmp/claude-1/x/s/tasks/.output."),
            // A Read of a file that quotes a notice is not an end record.
            line(json!({
                "type": "user",
                "message": {"role": "user", "content": [{"tool_use_id": "t9", "type": "tool_result", "content": "<task-notification>\n<task-id>b2</task-id>\n<status>killed</status>\n</task-notification>"}]},
            })),
            notification("b1", "completed"),
        ]
        .concat();

        let found = tasks("orchestrator", &log);
        let brief: Vec<_> = found
            .iter()
            .map(|t| (t.id.as_str(), t.kind, t.title.as_str(), t.status.as_str()))
            .collect();
        assert_eq!(
            brief,
            [
                ("b1", TaskKind::Shell, "Sleep for 30 seconds", "completed"),
                ("b2", TaskKind::Shell, "cargo test", "running"),
                ("a3", TaskKind::Agent, "Research speakers", "unknown"),
            ]
        );
        assert_eq!(
            found[0].summary.as_deref(),
            Some("Background command \"x\" completed")
        );
        assert_eq!(
            found[0].output.as_deref(),
            Some(Path::new("/tmp/claude-1/x/s/tasks/b1.output"))
        );
        assert_eq!(
            found[2].output.as_deref(),
            Some(Path::new("/tmp/claude-1/x/s/tasks/a3.output"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
