//! Codex mirror and thought joins, and turn rollback replay.
use super::super::transcript::{
    CODEX_MIRROR, CONTEXT_COMPACTED, Record, RowKind, meta, row, string,
};
use std::collections::HashMap;

pub(super) fn thought_parts(
    records: &[Record],
    scopes: &[usize],
    joined: &std::collections::HashSet<usize>,
) -> HashMap<usize, Vec<usize>> {
    let mut steps = Vec::with_capacity(records.len());
    let mut step = 0;

    let mut thoughts = HashMap::<(usize, usize, &str), Vec<usize>>::new();
    for (index, record) in records.iter().enumerate() {
        if !joined.contains(&index)
            && row(record)
                .is_some_and(|row| row.kind != RowKind::Thought && !super::is_collapsed(&row))
        {
            step += 1;
        }
        steps.push(step);

        if string(&record.event, "type") == "agent_thought_chunk" && !joined.contains(&index) {
            thoughts
                .entry((scopes[index], step, meta(&record.event, "session_id")))
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
            .entry((
                scopes[index],
                steps[index],
                meta(&record.event, "session_id"),
            ))
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
            || kind == "user_message_chunk" && !explicit && visible && event[CODEX_MIRROR] != true
        {
            if visible || explicit {
                turns.push(std::mem::take(&mut current));
            }
            current.clear();
            explicit = kind == "turn_started";
            visible = false;
        }
        current.push(index);
        visible |= !matches!(kind, "session_info" | "turn_ended")
            && row(record).is_some_and(|row| {
                !super::is_collapsed(&row) && row.system_kind.as_deref() != Some("system")
            });

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

pub(super) fn mirror_parts(records: &[Record], scopes: &[usize]) -> HashMap<usize, Vec<usize>> {
    let mut parts = HashMap::<usize, Vec<usize>>::new();
    let mut identities = HashMap::<(usize, &str, &str, &str, &str), Vec<usize>>::new();
    let mut texts = HashMap::<(usize, &str, &str, &str, &str), Vec<usize>>::new();
    let mut idless_texts = HashMap::<(usize, &str, &str, &str, &str), Vec<usize>>::new();
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;

        if event[CODEX_MIRROR] == true || !mirror_kind(record) {
            continue;
        }
        let base = (
            scopes[index],
            meta(event, "session_id"),
            string(event, "type"),
            string(event, "kind"),
        );

        let id = item_id(event);
        if !id.is_empty() {
            identities
                .entry((base.0, base.1, base.2, base.3, id))
                .or_default()
                .push(index);
        }

        let key = (base.0, base.1, base.2, base.3, string(event, "text"));
        texts.entry(key).or_default().push(index);
        if id.is_empty() {
            idless_texts.entry(key).or_default().push(index);
        }
    }
    // An extension image can have only completion copies, with no response item.
    // Use the first copy as its row when no canonical image has this identity.
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        let id = item_id(event);
        if string(event, "type") == "image" && event[CODEX_MIRROR] == true && !id.is_empty() {
            identities
                .entry((
                    scopes[index],
                    meta(event, "session_id"),
                    "image",
                    string(event, "kind"),
                    id,
                ))
                .or_insert_with(|| vec![index]);
        }
    }

    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        if event[CODEX_MIRROR] != true || !mirror_kind(record) {
            continue;
        }
        let base = (
            scopes[index],
            meta(event, "session_id"),
            string(event, "type"),
            string(event, "kind"),
        );

        let id = item_id(event);
        let identity = identities
            .get(&(base.0, base.1, base.2, base.3, id))
            .filter(|_| !id.is_empty());
        let text = if id.is_empty() { &texts } else { &idless_texts };
        let candidates = identity.or_else(|| {
            if matches!(
                base.2,
                "tool_call" | "subagent_activity" | "elicitation_result"
            ) {
                None
            } else {
                text.get(&(base.0, base.1, base.2, base.3, string(event, "text")))
            }
        });

        if let Some(target) =
            candidates.and_then(|items| items.iter().min_by_key(|&&other| other.abs_diff(index)))
            && *target != index
        {
            parts.entry(*target).or_default().push(index);
        }
    }

    let mut group: Vec<usize> = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        if string(event, "type") == "system_message" && string(event, "kind") == "compaction" {
            if group.first().is_some_and(|&first| {
                meta(&records[first].event, "session_id") != meta(event, "session_id")
            }) {
                join_compactions(records, &mut group, &mut parts);
            }
            group.push(index);
        } else if !(matches!(string(event, "type"), "ignored" | "usage" | "session_info")
            || string(event, "type") == "system_message"
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
            !text.is_empty() && text != CONTEXT_COMPACTED,
            !text.is_empty(),
            event[CODEX_MIRROR] != true,
        )
    });

    if let Some(target) = target {
        parts
            .entry(target)
            .or_default()
            .extend(group.drain(..).filter(|&index| index != target));
    }
}

fn mirror_kind(record: &Record) -> bool {
    let event = &record.event;
    match string(event, "type") {
        "tool_call" | "elicitation_result" => true,
        "system_message" => string(event, "kind") != "compaction",
        "agent_thought_part" => false,
        _ => row(record).is_some_and(|row| {
            matches!(
                row.kind,
                RowKind::User
                    | RowKind::Assistant
                    | RowKind::Thought
                    | RowKind::Subagent
                    | RowKind::ProposedPlan
            ) || row.kind == RowKind::Notice && string(event, "type") == "image"
        }),
    }
}

fn item_id(event: &serde_json::Value) -> &str {
    match string(event, "type") {
        "tool_call" | "elicitation_result" => string(event, "tool_call_id"),
        "subagent_activity" => string(event, "agent_thread_id"),
        _ => meta(event, "uuid"),
    }
}
