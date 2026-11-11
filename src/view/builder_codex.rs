//! Match materialized item copies to the same rows from response items.

use super::super::transcript::{Record, meta, string};
use std::collections::HashMap;

pub(super) fn thought_parts(records: &[Record], scopes: &[usize]) -> HashMap<usize, Vec<usize>> {
    let mut thoughts = HashMap::<(usize, &str), Vec<usize>>::new();
    for (index, record) in records.iter().enumerate() {
        if string(&record.event, "type") == "agent_thought_chunk"
            && record.event["codex_mirror"] != true
        {
            thoughts
                .entry((scopes[index], meta(&record.event, "session_id")))
                .or_default()
                .push(index);
        }
    }
    let mut parts = HashMap::<usize, Vec<usize>>::new();
    for (index, record) in records.iter().enumerate() {
        if string(&record.event, "type") != "agent_thought_part" {
            continue;
        }
        let entries = thoughts
            .entry((scopes[index], meta(&record.event, "session_id")))
            .or_default();
        let target = entries
            .iter()
            .rev()
            .find(|&&other| other < index)
            .or_else(|| entries.first())
            .copied();
        if let Some(target) = target {
            parts.entry(target).or_default().push(index);
        } else {
            entries.push(index);
        }
    }
    parts
}

pub(super) fn rolled_back(records: &[Record]) -> Vec<Record> {
    let mut turns: Vec<Vec<usize>> = Vec::new();
    let mut current = Vec::new();
    let mut explicit = false;
    let mut visible = false;
    let mut removed = std::collections::HashSet::new();
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        let kind = string(event, "type");
        if kind == "thread_rolled_back" {
            if visible || explicit {
                turns.push(std::mem::take(&mut current));
            }
            current.clear();
            let count = event["num_turns"]
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
                .unwrap_or(usize::MAX);
            let keep = turns.len().saturating_sub(count);
            removed.extend(turns.drain(keep..).flatten());
            visible = false;
            explicit = false;
            continue;
        }
        if kind == "turn_started"
            || kind == "page"
            || kind == "user_message_chunk" && !explicit && visible && event["codex_mirror"] != true
        {
            if visible || explicit {
                turns.push(std::mem::take(&mut current));
            }
            current.clear();
            explicit = kind == "turn_started";
            visible = false;
        }
        current.push(index);
        visible |= matches!(
            kind,
            "user_message_chunk"
                | "agent_message_chunk"
                | "agent_thought_chunk"
                | "tool_call"
                | "tool_call_update"
                | "tool_diff"
                | "image"
                | "proposed_plan"
                | "subagent_activity"
                | "error"
        ) || kind == "system_message"
            && !matches!(
                string(event, "kind"),
                "developer" | "context" | "injected" | "system"
            );
        if kind == "turn_ended" {
            if visible || explicit {
                turns.push(std::mem::take(&mut current));
            }
            current.clear();
            explicit = false;
            visible = false;
        }
    }
    records
        .iter()
        .enumerate()
        .filter(|(index, _)| !removed.contains(index))
        .map(|(_, record)| record.clone())
        .collect()
}

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
        if event["codex_mirror"] != true
            || string(event, "type") == "tool_call_update"
            || string(event, "type") == "system_message" && string(event, "kind") == "compaction"
        {
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
    let mut group = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        if string(event, "type") == "system_message" && string(event, "kind") == "compaction" {
            group.push(index);
        } else if !matches!(string(event, "type"), "ignored" | "usage" | "session_info")
            && !(string(event, "type") == "system_message"
                && matches!(string(event, "kind"), "developer" | "context"))
        {
            join_compactions(records, &mut group, &mut parts);
        }
    }
    join_compactions(records, &mut group, &mut parts);
    parts
}

fn join_compactions(
    records: &[Record],
    group: &mut Vec<usize>,
    parts: &mut HashMap<usize, Vec<usize>>,
) {
    let target = group.iter().copied().max_by_key(|&index| {
        let event = &records[index].event;
        let text = string(event, "text");
        (
            !text.is_empty() && text != "Context compacted",
            !text.is_empty(),
            event["codex_mirror"] != true,
        )
    });
    if let Some(target) = target {
        parts
            .entry(target)
            .or_default()
            .extend(group.drain(..).filter(|&index| index != target));
    }
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
