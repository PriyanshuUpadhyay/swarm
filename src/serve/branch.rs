//! Branch a chat from one of its user messages. The new chat gets the chair's CLI session up to
//! that message, as a new session file the CLI resumes, and the message goes back to its draft.
//! The files of the workspace stay as they are.

use super::*;

const UNREADABLE: &str = "The chat's log has a line that is not JSON, so it cannot be branched";
const NO_HISTORY: &str = "There is nothing before this message to branch";

/// A Claude session file cut before `prefix` ends: every record moves to session `id`, the way
/// the Agent SDK's `forkSession` does. A file that a resume appended to holds an older session id
/// too, so that moves as well. A cut between a tool call and its result would leave a call that
/// Claude's API refuses, so it is refused here.
pub(super) fn cut_claude(prefix: &str, id: &str) -> Result<String, &'static str> {
    let mut open = std::collections::HashSet::new();
    let mut messages = false;
    let mut out = String::new();
    for line in prefix.lines().filter(|line| !line.trim().is_empty()) {
        let mut record: Value = serde_json::from_str(line).map_err(|_| UNREADABLE)?;
        for item in record["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
        {
            match item["type"].as_str() {
                Some("tool_use") => {
                    open.insert(item["id"].as_str().unwrap_or_default().to_owned());
                }
                Some("tool_result") => {
                    open.remove(item["tool_use_id"].as_str().unwrap_or_default());
                }
                _ => {}
            }
        }
        messages |= record["message"].is_object();
        if record.get("sessionId").is_some() {
            record["sessionId"] = id.into();
        }
        out.push_str(&record.to_string());
        out.push('\n');
    }
    if !messages {
        return Err(NO_HISTORY);
    }
    if !open.is_empty() {
        return Err("This message came while a tool ran. Branch from an earlier message.");
    }
    Ok(out)
}

/// A pi session file with only the path from its root to the parent of the user entry at
/// `offset`, as pi's own `createBranchedSession` writes it: labels drop out and the path is
/// chained again. pi writes the labels of the path back as new entries; a branch goes without.
pub(super) fn cut_pi(
    text: &str,
    offset: usize,
    id: &str,
    parent_file: &str,
) -> Result<String, &'static str> {
    let (before, rest) = (text.get(..offset), text.get(offset..));
    let (Some(before), Some(rest)) = (before, rest) else {
        return Err(NO_HISTORY);
    };
    let user: Value =
        serde_json::from_str(rest.lines().next().unwrap_or_default()).map_err(|_| UNREADABLE)?;
    if user["type"] != "message" || user["message"]["role"] != "user" {
        return Err("Only a message you sent can start a branch");
    }
    let mut header = None;
    let mut entries = HashMap::new();
    for line in before.lines().filter(|line| !line.trim().is_empty()) {
        let entry: Value = serde_json::from_str(line).map_err(|_| UNREADABLE)?;
        if entry["type"] == "session" {
            header.get_or_insert(entry);
        } else if let Some(key) = entry["id"].as_str() {
            entries.insert(key.to_owned(), entry);
        }
    }
    let header = header.ok_or(UNREADABLE)?;
    let mut path = Vec::new();
    let mut at = user["parentId"].as_str();
    while let Some(key) = at {
        let entry = entries
            .remove(key)
            .ok_or("The chat's log is missing an earlier entry")?;
        path.push(entry);
        at = path
            .last()
            .and_then(|entry: &Value| entry["parentId"].as_str());
    }
    if path.is_empty() {
        return Err(NO_HISTORY);
    }
    path.reverse();
    let mut out = json!({
        "type": "session",
        "version": header["version"],
        "id": id,
        "timestamp": header["timestamp"],
        "cwd": header["cwd"],
        "parentSession": parent_file,
    })
    .to_string();
    out.push('\n');
    let (mut replaced, mut pending, mut parent) = (HashMap::new(), Vec::new(), Value::Null);
    for mut entry in path {
        let key = entry["id"].as_str().unwrap_or_default().to_owned();
        if entry["type"] == "label" {
            pending.push(key);
            continue;
        }
        for label in pending.drain(..) {
            replaced.insert(label, key.clone());
        }
        if entry["type"] == "compaction"
            && let Some(kept) = entry["firstKeptEntryId"]
                .as_str()
                .and_then(|k| replaced.get(k))
        {
            entry["firstKeptEntryId"] = kept.clone().into();
        }
        entry["parentId"] = std::mem::replace(&mut parent, key.into());
        out.push_str(&entry.to_string());
        out.push('\n');
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BranchChat {
    session: String,
    /// The id of a user row, which names the log line it came from: `user@raw-<offset>-<n>`.
    row: String,
}

/// The byte offset of the log line that `source` (`raw-<offset>-<n>`) came from.
fn line_offset(source: &str) -> Option<usize> {
    source.strip_prefix("raw-")?.split('-').next()?.parse().ok()
}

/// Starts a new chat in the same folder from the chat's history before one of its user messages
/// (ADR 0114). It returns the new chat and the message, which the app puts in the new draft.
pub(super) async fn branch_chat(
    State(app): State<App>,
    body: Result<Json<BranchChat>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| error(StatusCode::BAD_REQUEST, &e.body_text()))?;
    let old = body.session.as_str();
    let call = action_session(&app, old)?;
    let provider: Option<String> = read_connection(&app.root)
        .and_then(|c| {
            c.query_row(
                "SELECT chair_provider FROM session WHERE id = ?1",
                [old],
                |row| row.get(0),
            )
        })
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?;
    let provider = match provider.as_deref() {
        Some(cli @ ("claude" | "pi")) => cli.to_owned(),
        _ => {
            return Err(error(
                StatusCode::CONFLICT,
                "Only a Claude or pi chat can branch",
            ));
        }
    };
    let rows = handoff_rows(&app, old).await?;
    // The page's row id ends with `@<anchor>`, which the rows stream adds (view_rows_with_context).
    let anchor = body.row.rsplit_once('@').map_or("", |(_, anchor)| anchor);
    let mut users = rows.iter().filter(|row| row.kind == view::RowKind::User);
    let first = users.clone().next().map(|row| row.anchor_source_id.clone());
    let row = users
        .find(|row| !anchor.is_empty() && row.anchor_source_id == anchor)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown message"))?;
    let offset = line_offset(&row.anchor_source_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown message"))?;
    let text = row.text.clone();
    let log = resolve_log(&app.root, old, "chair")?;
    let chair = chair_agent(&app, &call).await?;

    let mut new = CliCall {
        args: Vec::new(),
        cwd: call.cwd.clone(),
        session: None,
        adapter: "tmux-swarm".into(),
    };
    let create = app.chat_create.lock().await;
    let output = run_cli(&app, new.args(&["session", "new", "lane"]), None).await?;
    drop(create);
    let session = output.trim().to_owned();
    if uuid::Uuid::parse_str(&session).is_err() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "swarm returned an invalid session id",
        ));
    }
    new.session = Some(session.clone());
    // A branch from the first message has no history, so its chat starts new.
    let mut launch = vec!["launch", "orchestrator", "chat"];
    if first.as_deref() == Some(row.anchor_source_id.as_str()) {
        run_cli(&app, new.args(&["session", "branch", &session, old]), None).await?;
        launch.extend(["--provider", &provider]);
    } else {
        let id = match provider.as_str() {
            // pi's status feed finds a chair's file by this name (hooks.rs).
            "pi" => format!("{session}-orchestrator"),
            _ => uuid::Uuid::now_v7().to_string(),
        };
        let (path, id_for_file, cli) = (log.path.clone(), id.clone(), provider.clone());
        tokio::task::spawn_blocking(move || write_cut(&path, offset, &cli, &id_for_file))
            .await
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "branch task failed"))??;
        let chair_arg = format!("{provider}:{id}");
        run_cli(
            &app,
            new.args(&["session", "branch", &session, old, &chair_arg]),
            None,
        )
        .await?;
        launch.push("--resume");
    }
    for (flag, key) in [
        ("--model", "model"),
        ("--effort", "effort"),
        ("--account", "account"),
    ] {
        if let Some(value) = chair[key].as_str() {
            launch.extend([flag, value]);
        }
    }
    let output = run_cli(&app, new.args(&launch), None).await?;
    let pane = first_line(&output).unwrap_or_default();
    Ok(Json(
        json!({"ok": true, "session": session, "agent": "orchestrator", "pane": pane, "text": text}),
    ))
}

/// Writes the cut of the log at `path` next to it, as the file the CLI resumes for `id`.
fn write_cut(path: &Path, offset: usize, provider: &str, id: &str) -> Result<(), ApiError> {
    use std::io::Write;
    let text = std::fs::read_to_string(path)
        .map_err(|_| error(StatusCode::CONFLICT, "The chat's log could not be read"))?;
    let refused = |reason: &str| error(StatusCode::CONFLICT, reason);
    let dir = path
        .parent()
        .ok_or_else(|| refused("The chat's log has no folder"))?;
    let (cut, name) = if provider == "pi" {
        let parent = path.to_string_lossy();
        // pi names a file `<time>_<id>.jsonl`; the branch keeps the time of the chat it came from.
        let time = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.split_once('_'))
            .map_or("branch", |(time, _)| time);
        (
            cut_pi(&text, offset, id, &parent).map_err(refused)?,
            format!("{time}_{id}.jsonl"),
        )
    } else {
        let prefix = text
            .get(..offset)
            .ok_or_else(|| refused("unknown message"))?;
        (
            cut_claude(prefix, id).map_err(refused)?,
            format!("{id}.jsonl"),
        )
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(name))
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    file.write_all(cut.as_bytes())
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))
}

#[cfg(test)]
#[path = "branch/tests.rs"]
mod tests;
