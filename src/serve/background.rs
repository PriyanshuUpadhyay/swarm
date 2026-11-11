//! A chat's background tasks: the shells and subagents its live Claude agents started, the
//! background terminals its live Codex agents started, and the background tasks its live AGY
//! agents started (ADR 0104).
//!
//! Claude Code runs them inside its own process, so nothing outside can stop them or type into
//! them. Its log names each one: a tool result with `toolUseResult.backgroundTaskId` (a shell) or
//! `toolUseResult.isAsync` (a subagent), and later a `<task-notification>` with the end state.

use super::*;
use swarm::providers::Provider;

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
    /// The log record's own timestamp for the launch, for the chat's running timer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) started_at: Option<String>,
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
            title: title_of(&title),
            status: String::new(),
            summary: None,
            started_at: record["timestamp"].as_str().map(str::to_string),
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

/// Every background terminal one live Codex agent's log names, oldest first. Codex unified exec
/// answers a command that outlives its wait with `Process running with session ID <n>`; the
/// session is running until a later `write_stdin` poll answers `Process exited with code <c>`.
/// Only the header, the text before `Output:`, is read: the command's own output may print either
/// phrase.
pub(super) fn codex_tasks(agent: &str, log: &str) -> Vec<Task> {
    // The command and the log time of the call, which is the launch; its answer comes after the wait.
    let mut commands: HashMap<String, (String, Option<String>)> = HashMap::new();
    let mut polls: HashMap<String, String> = HashMap::new();
    let mut found: Vec<Task> = Vec::new();

    for line in log.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = &record["payload"];
        let call = payload["call_id"].as_str().unwrap_or("");

        match payload["type"].as_str() {
            Some("function_call") => {
                let args: Value = payload["arguments"]
                    .as_str()
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or_default();
                match payload["name"].as_str() {
                    Some("exec_command") => {
                        let cmd = args["cmd"].as_str().unwrap_or("");
                        let at = record["timestamp"].as_str().map(str::to_string);
                        commands.insert(call.into(), (cmd.into(), at));
                    }
                    Some("write_stdin") => {
                        if let Some(id) = args["session_id"].as_i64() {
                            polls.insert(call.into(), id.to_string());
                        }
                    }
                    _ => {}
                }
            }
            Some("function_call_output") => {
                let Some(text) = payload["output"].as_str() else {
                    continue;
                };
                let header = text.split_once("Output:").map_or(text, |(head, _)| head);

                if let Some(id) = number_after(header, "Process running with session ID ") {
                    if let Some((cmd, started_at)) = commands.get(call)
                        && !found.iter().any(|task| task.id == id)
                    {
                        found.push(Task {
                            id,
                            agent: agent.into(),
                            kind: TaskKind::Shell,
                            title: title_of(cmd),
                            status: "running".into(),
                            summary: None,
                            started_at: started_at.clone(),
                            output: None,
                        });
                    }
                } else if let Some(id) = polls.get(call)
                    && let Some(code) = number_after(header, "Process exited with code ")
                    && let Some(task) = found.iter_mut().find(|task| task.id == *id)
                {
                    task.status = if code == "0" { "completed" } else { "failed" }.into();
                }
            }
            _ => {}
        }
    }

    found
}

/// Every background task one live AGY agent's log names, oldest first. A `GENERIC` step with status
/// `RUNNING` says `Tool is running as a background task with task id: <conv>/task-<n>` and holds a
/// `Task Description:` line; a later `SYSTEM_MESSAGE` step with `sender=<that id>` ends it. The
/// task's log file lies outside the folder that the panel's read-only view opens (ADR 0104), so the
/// row has no log.
pub(super) fn agy_tasks(agent: &str, log: &str) -> Vec<Task> {
    const START: &str = "Tool is running as a background task with task id: ";
    let mut found: Vec<Task> = Vec::new();

    for line in log.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(text) = record["content"].as_str() else {
            continue;
        };

        if record["type"] == "GENERIC" && record["status"] == "RUNNING" {
            let Some(id) = text
                .split_once(START)
                .and_then(|(_, rest)| rest.lines().next())
                .map(str::trim)
                .filter(|id| !id.is_empty() && !found.iter().any(|task| task.id == *id))
            else {
                continue;
            };
            let described = text
                .split_once("Task Description: ")
                .map_or("", |(_, rest)| rest);

            found.push(Task {
                id: id.into(),
                agent: agent.into(),
                kind: TaskKind::Shell,
                title: title_of(described),
                status: "running".into(),
                summary: None,
                started_at: record["created_at"].as_str().map(str::to_string),
                output: None,
            });
        } else if record["type"] == "SYSTEM_MESSAGE"
            && let Some((_, rest)) = text.split_once("sender=")
            && let Some(sender) = rest.split_whitespace().next()
            && let Some(task) = found.iter_mut().find(|task| task.id == sender)
        {
            // A timer (`schedule`) ends with its prompt and no exit code.
            let failed =
                number_after(text, "Command exited with code ").is_some_and(|code| code != "0");
            task.status = if failed { "failed" } else { "completed" }.into();
        }
    }

    found
}

/// The first whitespace-separated word after `label`, when it is a number.
fn number_after(text: &str, label: &str) -> Option<String> {
    let word = text[text.find(label)? + label.len()..]
        .split_whitespace()
        .next()?;
    word.parse::<i64>().ok().map(|n| n.to_string())
}

/// A task title keeps its first line, cut to `TITLE_CHARS` characters.
fn title_of(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(TITLE_CHARS)
        .collect()
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

/// A reader of one provider's log: the agent id and the log text give its background tasks.
type Reader = fn(&str, &str) -> Vec<Task>;

/// The background reader of a provider, or `None` when it has no background work. The match is
/// exhaustive, so a new provider shows here.
fn reader(provider: Provider) -> Option<Reader> {
    match provider {
        Provider::Claude => Some(tasks),
        Provider::Codex => Some(codex_tasks),
        Provider::Agy => Some(agy_tasks),
        Provider::Pi => None,
    }
}

/// The live agents of a session that have a background reader, as (agent id, reader, log path).
fn agent_logs(app: &Server, session: &str) -> Vec<(String, Reader, PathBuf)> {
    let hub = app.hub.lock().unwrap();

    live_agents(&hub.agents[session])
        .filter_map(|agent| {
            let read = reader(Provider::parse(agent["provider"].as_str()?)?)?;
            let log = agent["log"].as_str().filter(|log| !log.is_empty())?;

            Some((agent["id"].as_str()?.to_string(), read, PathBuf::from(log)))
        })
        .collect()
}

/// Every background task of the session's live agents.
pub(super) fn session_tasks(app: &Server, session: &str) -> Vec<Task> {
    agent_logs(app, session)
        .into_iter()
        .flat_map(|(agent, read, log)| {
            // ponytail: reads each whole log per call; keep an offset per log if a long chat lags.
            let text = std::fs::read_to_string(log).unwrap_or_default();
            read(&agent, &text)
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
#[path = "background/tests.rs"]
mod tests;
