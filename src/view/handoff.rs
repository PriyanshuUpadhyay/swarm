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

/// None while the summary turn runs. Some(None) when it ended with no summary, such as a usage
/// limit error, so the caller stops waiting and lets the new agent summarize.
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
    let text = rows
        .iter()
        .filter(|r| matches!(r.kind, RowKind::User | RowKind::Assistant))
        .map(|r| {
            let speaker = if r.kind == RowKind::User {
                "User"
            } else {
                "Agent"
            };
            format!("{speaker}: {}", r.text.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

/// The longest chat text the first message carries itself; a longer chat goes in a file.
pub const INLINE_LIMIT: usize = 12_000;

pub fn first_message(context: &str, summary: bool) -> String {
    let (heading, instruction) = if summary {
        (
            "The prior agent prepared this compact handoff summary:",
            "Use this context for the next request. Confirm that you are ready, then wait.",
        )
    } else {
        (
            "These are the messages of the prior chat:",
            "Make a compact working summary from this context. Confirm that you are ready, then wait.",
        )
    };
    format!("{heading}\n\n{context}\n\n{instruction}")
}

/// The first message when the whole prior chat is in the file at `path`.
pub fn file_message(path: &str) -> String {
    format!(
        "The prior chat is too long to paste, so its user and agent messages are in {path}. Read all of it, then make a compact working summary. Confirm that you are ready, then wait."
    )
}
