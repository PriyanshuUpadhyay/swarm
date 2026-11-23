//! Turn scopes, and how a later record finds the call it belongs to.
use super::super::transcript::meta;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct Anchor {
    pub(super) index: usize,
    pub(super) session: String,
    pub(super) scope: usize,
}
pub(super) fn anchor_match(
    id: &str,
    event: &Value,
    scope: usize,
    anchors: &HashMap<String, Vec<Anchor>>,
) -> Option<usize> {
    if id.is_empty() {
        return None;
    }
    let session = meta(event, "session_id");
    let mut matches = anchors.get(id)?.iter().filter(|anchor| {
        anchor.scope == scope
            && (session.is_empty() || anchor.session.is_empty() || session == anchor.session)
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first.index)
}
pub(crate) fn starts_scope(kind: &str, saw_tool: bool) -> bool {
    matches!(kind, "page" | "turn_started") || kind == "user_message_chunk" && saw_tool
}

pub(crate) fn is_tool_event(kind: &str) -> bool {
    matches!(kind, "tool_call" | "tool_call_update" | "tool_diff")
}
