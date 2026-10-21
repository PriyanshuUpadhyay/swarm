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

pub fn completed_summary(rows: &[Row], baseline: usize) -> Option<String> {
    let added = rows.get(baseline..)?;
    let question = added
        .iter()
        .position(|r| r.kind == RowKind::User && r.text.contains(REQUEST))?;
    let end = question + added[question..].iter().position(|r| r.ends_turn)?;
    if added[end].kind != RowKind::Result || added[end].text != "completed" {
        return None;
    }
    let answer = added[question..end]
        .iter()
        .rev()
        .find(|r| r.kind == RowKind::Assistant)?
        .text
        .trim();
    (!answer.is_empty()).then(|| answer.chars().take(12_000).collect())
}

pub fn recent_context(rows: &[Row]) -> Option<String> {
    let messages: Vec<_> = rows
        .iter()
        .filter(|r| matches!(r.kind, RowKind::User | RowKind::Assistant))
        .collect();
    if messages.is_empty() {
        return None;
    }
    let text = messages[messages.len().saturating_sub(16)..]
        .iter()
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
    let skip = text.chars().count().saturating_sub(12_000);
    Some(text.chars().skip(skip).collect())
}

pub fn first_message(context: &str, summary: bool) -> String {
    let (heading, instruction) = if summary {
        (
            "The prior agent prepared this compact handoff summary:",
            "Use this context for the next request. Confirm that you are ready, then wait.",
        )
    } else {
        (
            "These are recent messages from the prior chat:",
            "Make a compact working summary from this context. Confirm that you are ready, then wait.",
        )
    };
    format!("{heading}\n\n{context}\n\n{instruction}")
}
