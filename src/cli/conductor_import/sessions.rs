//! The sessions that `swarm import conductor` rebuilds from Conductor's rows, in each CLI's own
//! format and folder (ADR 0107).

use super::*;

/// Claude's config folder, which holds its transcripts.
pub(super) fn claude_config() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(match env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env_var("HOME")?).join(".claude"),
    })
}

/// The folder Claude files a transcript of `cwd` under: the real path, each character other than
/// a letter or a digit made a `-`. `claude --resume` looks for the id there.
pub(super) fn claude_project(cwd: &Path) -> String {
    let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// `<y>/<m>/<d>/rollout-<y>-<m>-<d>T<h>-<m>-<s>-<id>.jsonl` at `at` in UTC, where Codex files a
/// session and finds it by the id at the end.
pub(super) fn rollout_name(conductor: &Connection, at: i64, id: &str) -> String {
    let stamp: String = conductor
        .query_row(
            "SELECT strftime('%Y/%m/%d/rollout-%Y-%m-%dT%H-%M-%S', ?1, 'unixepoch')",
            [at],
            |row| row.get(0),
        )
        .unwrap_or_default();
    format!("{stamp}-{id}.jsonl")
}

/// One Claude-shaped record from one Conductor message row, or None for a row that has no place
/// in a transcript: a status event, or a step of a subagent, which Claude keeps in another file.
pub(super) fn log_line(role: &str, content: &str, id: &str, at: &str) -> Option<Value> {
    if role == "user" {
        return Some(json!({
            "type": "user",
            "uuid": id,
            "timestamp": at,
            "message": {"role": "user", "content": content},
        }));
    }
    let mut event: Value = serde_json::from_str(content).ok()?;
    let record = event.as_object_mut()?;
    if !matches!(record.get("type")?.as_str()?, "assistant" | "user")
        || record
            .get("parent_tool_use_id")
            .is_some_and(|parent| !parent.is_null())
    {
        return None;
    }
    record.remove("session_id");
    record.entry("uuid").or_insert(id.into());
    record.entry("timestamp").or_insert(at.into());
    if let Some(result) = record.remove("tool_use_result") {
        record.entry("toolUseResult").or_insert(result);
    }
    Some(event)
}

/// The chat's records in order, as `log_line` makes them.
pub(super) fn records(conductor: &Connection, chat: &Chat) -> Result<Vec<Value>, String> {
    let mut statement = conductor
        .prepare(
            "SELECT id, coalesce(role, ''), coalesce(content, ''),
                    coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', created_at), created_at, '')
             FROM session_messages WHERE session_id = ?1 AND cancelled_at IS NULL
             ORDER BY created_at, rowid",
        )
        .map_err(schema_error)?;
    let mut records = Vec::new();
    let mut rows = statement
        .query([&chat.conductor_id])
        .map_err(schema_error)?;
    while let Some(row) = rows.next().map_err(schema_error)? {
        let get = |i| row.get::<_, String>(i).map_err(schema_error);
        records.extend(log_line(&get(1)?, &get(2)?, &get(0)?, &get(3)?));
    }
    Ok(records)
}

/// A transcript that `claude --resume <id>` takes: each record names the session, the folder,
/// and the record before it.
pub(super) fn claude_log(records: Vec<Value>, cwd: &str, id: &str) -> String {
    let mut parent = Value::Null;
    let mut text = String::new();
    for mut record in records {
        let uuid = record["uuid"].clone();
        if let Some(object) = record.as_object_mut() {
            object.insert("parentUuid".into(), parent);
            object.insert("isSidechain".into(), false.into());
            object.insert("userType".into(), "external".into());
            object.insert("cwd".into(), cwd.into());
            object.insert("sessionId".into(), id.into());
        }
        parent = uuid;
        text.push_str(&record.to_string());
        text.push('\n');
    }
    text
}

/// A rollout that `codex resume <id>` takes, from the Claude-shaped records Conductor keeps of a
/// Codex chat. Conductor kept no reasoning and named each shell call `Bash`, so the rollout has no
/// reasoning and names a shell call `exec_command` again.
pub(super) fn codex_log(records: Vec<Value>, cwd: &str, id: &str) -> String {
    let started = records
        .first()
        .map_or(Value::Null, |record| record["timestamp"].clone());
    let mut text = String::new();
    let mut line = |at: &Value, kind: &str, payload: Value| {
        text.push_str(&json!({"timestamp": at, "type": kind, "payload": payload}).to_string());
        text.push('\n');
    };
    line(
        &started,
        "session_meta",
        json!({"id": id, "timestamp": started, "cwd": cwd, "originator": "swarm_import",
               "cli_version": "0.0.0", "source": "cli", "model_provider": "openai"}),
    );
    for record in &records {
        let at = &record["timestamp"];
        let user = record["type"] == "user";
        let content = &record["message"]["content"];
        let blocks = match content.as_str() {
            Some(prompt) => vec![json!({"type": "text", "text": prompt})],
            None => content.as_array().cloned().unwrap_or_default(),
        };
        for block in blocks {
            match block["type"].as_str() {
                Some("text") if user => {
                    let said = &block["text"];
                    let input = json!([{"type": "input_text", "text": said}]);
                    line(
                        at,
                        "response_item",
                        json!({"type": "message", "role": "user", "content": input}),
                    );
                    line(
                        at,
                        "event_msg",
                        json!({"type": "user_message", "message": said, "images": []}),
                    );
                }
                Some("text") => {
                    let said = &block["text"];
                    let output = json!([{"type": "output_text", "text": said}]);
                    line(
                        at,
                        "response_item",
                        json!({"type": "message", "role": "assistant", "content": output}),
                    );
                    line(
                        at,
                        "event_msg",
                        json!({"type": "agent_message", "message": said}),
                    );
                }
                Some("tool_use") => {
                    let input = &block["input"];
                    let (name, arguments) =
                        match (block["name"].as_str(), input["command"].as_str()) {
                            (Some("Bash"), Some(command)) => {
                                ("exec_command", json!({"cmd": command}))
                            }
                            (name, _) => (name.unwrap_or_default(), input.clone()),
                        };
                    line(
                        at,
                        "response_item",
                        json!({"type": "function_call", "name": name,
                        "arguments": arguments.to_string(), "call_id": block["id"]}),
                    );
                }
                Some("tool_result") => {
                    let output = match &block["content"] {
                        Value::String(output) => output.clone(),
                        parts => parts
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|part| part["text"].as_str())
                            .collect(),
                    };
                    line(
                        at,
                        "response_item",
                        json!({"type": "function_call_output",
                        "call_id": block["tool_use_id"], "output": output}),
                    );
                }
                _ => {}
            }
        }
    }
    text
}

#[cfg(test)]
#[path = "sessions/tests.rs"]
mod tests;
