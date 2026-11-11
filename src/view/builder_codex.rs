//! Match materialized item copies to the same rows from response items.

use super::super::transcript::{Record, meta, string};
use std::collections::HashMap;

pub(super) fn mirror_parts(records: &[Record]) -> HashMap<usize, Vec<usize>> {
    let mut parts = HashMap::<usize, Vec<usize>>::new();
    let mut scopes = Vec::with_capacity(records.len());
    let mut scope = 0;
    for record in records {
        if matches!(string(&record.event, "type"), "turn_started" | "page") {
            scope += 1;
        }
        scopes.push(scope);
        if string(&record.event, "type") == "turn_ended" {
            scope += 1;
        }
    }
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        if event["codex_mirror"] != true || string(event, "type") == "tool_call_update" {
            continue;
        }
        let candidate = records
            .iter()
            .enumerate()
            .filter(|(other, candidate)| {
                let target = &candidate.event;
                *other != index
                    && scopes[*other] == scopes[index]
                    && target["codex_mirror"] != true
                    && meta(target, "session_id") == meta(event, "session_id")
                    && same_item(event, target)
            })
            .min_by_key(|(other, _)| other.abs_diff(index))
            .map(|(other, _)| other);
        if let Some(target) = candidate {
            parts.entry(target).or_default().push(index);
        }
    }
    parts
}

fn same_item(event: &serde_json::Value, target: &serde_json::Value) -> bool {
    let kind = string(event, "type");
    if kind != string(target, "type") {
        return false;
    }
    if kind == "tool_call" {
        let id = string(event, "tool_call_id");
        return !id.is_empty() && id == string(target, "tool_call_id");
    }
    if kind == "subagent_activity" {
        return string(event, "agent_thread_id") == string(target, "agent_thread_id")
            && string(event, "kind") == string(target, "kind");
    }
    if kind == "unknown" || kind == "tool_diff" {
        return false;
    }
    if !matches!(
        kind,
        "user_message_chunk"
            | "agent_message_chunk"
            | "agent_thought_chunk"
            | "image"
            | "system_message"
            | "proposed_plan"
    ) {
        return false;
    }
    if string(event, "kind") != string(target, "kind") {
        return false;
    }
    let (id, target_id) = (meta(event, "uuid"), meta(target, "uuid"));
    if !id.is_empty() && !target_id.is_empty() {
        return id == target_id;
    }
    string(event, "text") == string(target, "text")
}
