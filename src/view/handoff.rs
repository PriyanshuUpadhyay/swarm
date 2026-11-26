use super::transcript::{Row, RowKind};
use serde_json::Value;

pub const REQUEST: &str = "Write a compact handoff summary for the next agent. Include the goal, decisions, work done, open tasks, and essential file paths. Do not start new work.";

pub fn ready(session: &Value, provider: &str) -> bool {
    session["chair_provider"] == provider && session["chair_id"].is_string()
}

/// An AGY chair learns its conversation id only from a turn's hook, so before the summary it is
/// ready when `agents --json` reads its pane as idle at the prompt.
pub fn agy_ready(agents: &Value) -> bool {
    agents["agents"].as_array().is_some_and(|list| {
        list.iter()
            .any(|a| a["id"] == "orchestrator" && a["alive"] == true && a["state"] == "done")
    })
}

/// None while the summary turn runs. Some(None) when it ended without a summary.
pub fn summary_turn(rows: &[Row], baseline: usize) -> Option<Option<String>> {
    let added = rows.get(baseline..)?;
    let question = added
        .iter()
        .position(|r| r.kind == RowKind::User && r.text.contains(REQUEST))?;
    let end = question + added[question..].iter().position(|r| r.ends_turn)?;
    if added[end].kind != RowKind::Result || added[end].text != "completed" {
        return Some(None);
    }
    let answer = added[question..end]
        .iter()
        .rev()
        .find(|r| r.kind == RowKind::Assistant)
        .map_or("", |r| r.text.trim());
    Some((!answer.is_empty()).then(|| answer.chars().take(12_000).collect()))
}

/// Each user and agent message of the chat, oldest first, or None when it has none.
pub fn chat_text(rows: &[Row]) -> Option<String> {
    let mut text = Vec::new();
    let mut synthetic = false;
    for row in rows {
        let body = row.text.trim();
        match row.kind {
            RowKind::User => {
                synthetic = body == REQUEST
                    || body.starts_with(SUMMARY_HEADING)
                    || body.starts_with(FILE_HEADING)
                    || body.starts_with("The prior chat is too long to paste, so its user and agent messages are in ")
                    || body.starts_with("These are the messages of the prior chat:\n\n");
                if !synthetic {
                    text.push(format!("User: {body}"));
                }
            }
            RowKind::Assistant if !synthetic => text.push(format!("Agent: {body}")),
            _ => {}
        }
    }
    (!text.is_empty()).then(|| text.join("\n\n"))
}

const SUMMARY_HEADING: &str = "The prior agent prepared this compact handoff summary:\n\n";
const FILE_HEADING: &str = "The prior chat's user and agent messages are in ";

pub fn summary_message(context: &str) -> String {
    format!(
        "{SUMMARY_HEADING}{context}\n\nUse this context for the next request. Confirm that you are ready, then wait."
    )
}

/// The first message when the whole prior chat is in the file at `path`.
pub fn file_message(path: &str) -> String {
    format!(
        "{FILE_HEADING}{path}. Read all of it, then make a compact working summary. Confirm that you are ready, then wait."
    )
}
