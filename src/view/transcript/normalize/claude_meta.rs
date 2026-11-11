//! Claude metadata and text classification from the provider record.

use super::NormalizedEvent;
use super::claude_json::Object;
use serde_json::{Value, json};

pub(super) fn meta(rec: &Object<'_>) -> Value {
    json!({
        "session_id": rec.text("sessionId"), "uuid": rec.text("uuid"),
        "timestamp": rec.text("timestamp"), "parent_uuid": rec.optional_text("parentUuid"),
        "source_tool_use_id": rec.optional_text("sourceToolUseID"),
    })
}

fn event_value(kind: &str, meta: &Value, fields: Value) -> Value {
    let mut result = serde_json::Map::new();
    result.insert("type".into(), kind.into());
    if let Value::Object(fields) = fields {
        result.extend(fields);
    }
    result.insert("meta".into(), meta.clone());
    Value::Object(result)
}

pub(super) fn event(kind: &str, meta: &Value, fields: Value) -> NormalizedEvent {
    NormalizedEvent::new(&event_value(kind, meta, fields))
}

pub(super) fn unknown(line: &[u8], meta: &Value) -> NormalizedEvent {
    event(
        "unknown",
        meta,
        json!({"raw": String::from_utf8_lossy(line)}),
    )
}

pub(super) fn ignored(kind: &str, line: &[u8], meta: &Value) -> NormalizedEvent {
    event(
        "ignored",
        meta,
        json!({"kind": kind, "raw": String::from_utf8_lossy(line)}),
    )
}

pub(super) fn ring(text: &str) -> bool {
    let mut any = false;
    for line in text.split('\n') {
        let line = line.trim_matches([' ', '\t', '\r']);
        if line.is_empty() {
            continue;
        }
        if !line.starts_with("swarm: new message. Run swarm inbox") {
            return false;
        }
        any = true;
    }
    any
}

fn starts(text: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| text.starts_with(prefix))
}

pub(super) fn text_kind(rec: &Object<'_>, text: &str) -> &'static str {
    if rec.boolean("isCompactSummary") == Some(true) {
        return "compact_summary";
    }
    if ring(text) {
        return "swarm_ring";
    }
    let trimmed = text.trim_start_matches([' ', '\t', '\r', '\n']);
    if starts(
        trimmed,
        &["<command-name>", "<command-message>", "<command-args>"],
    ) {
        return "command";
    }
    if starts(
        trimmed,
        &["<local-command-stdout>", "<local-command-stderr>"],
    ) {
        return "command_output";
    }
    if trimmed.starts_with("<task-notification>") {
        return "task_notification";
    }
    if trimmed.starts_with("<bash-input>") {
        return "shell_input";
    }
    if starts(trimmed, &["<bash-stdout>", "<bash-stderr>"]) {
        return "shell_output";
    }
    let is_meta = rec.boolean("isMeta") == Some(true);
    if is_meta && trimmed.starts_with("Base directory for this skill:") {
        return "skill_body";
    }
    if trimmed.starts_with("[Request interrupted by user") {
        return "interrupted";
    }
    if starts(
        trimmed,
        &[
            "<system-reminder>",
            "<fork-boilerplate",
            "<local-command-caveat>",
        ],
    ) {
        return "injected";
    }
    let from_agent = rec
        .object("origin")
        .and_then(|origin| origin.optional_text("kind"))
        .is_some_and(|kind| kind != "human");
    if from_agent || starts(trimmed, &["<teammate-message", "<cross-session-message"]) {
        return "peer_message";
    }
    if is_meta {
        return "injected";
    }
    ""
}

pub(super) fn text_event(
    meta: &Value,
    rec: &Object<'_>,
    is_user: bool,
    text: &str,
) -> NormalizedEvent {
    if is_user {
        let kind = text_kind(rec, text);
        if !kind.is_empty() {
            return event("system_message", meta, json!({"kind": kind, "text": text}));
        }
    }
    event(
        if is_user {
            "user_message_chunk"
        } else {
            "agent_message_chunk"
        },
        meta,
        json!({"text": text}),
    )
}

pub(super) fn usage(meta: &Value, kind: &str) -> Value {
    event_value(
        "usage",
        meta,
        json!({
            "source": "claude", "kind": kind, "context_tokens": null,
            "context_capacity_tokens": null, "input_tokens": null, "output_tokens": null,
            "cache_read_tokens": null, "cache_write_tokens": null,
            "session_input_tokens": null, "session_output_tokens": null,
            "cost_usd": null, "cost_completeness": "unknown",
        }),
    )
}
