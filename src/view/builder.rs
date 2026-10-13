use super::tool;
use super::transcript::{Record, Row, RowKind, chip, meta, row, shell_row, string};
use super::{TextPreview, ToolActivity, ToolState};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct Anchor {
    index: usize,
    session: String,
    scope: usize,
}
fn anchor_match(
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
pub fn rows(records: &[Record], has_older: bool, is_codex: bool) -> Vec<Row> {
    rows_with_context(records, has_older, is_codex, None)
}
pub(super) fn rows_with_context(
    records: &[Record],
    has_older: bool,
    is_codex: bool,
    context: Option<(bool, bool)>,
) -> Vec<Row> {
    let mut scope = 0;
    let mut saw_tool = false;
    let mut scopes = Vec::new();
    let mut calls = HashMap::<String, Vec<Anchor>>::new();
    let mut skill_calls = HashMap::<String, Vec<Anchor>>::new();
    let mut commands = HashMap::<String, Vec<Anchor>>::new();
    let mut shells = HashMap::<String, Vec<Anchor>>::new();
    let mut endings = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        let event_type = string(event, "type");
        if matches!(event_type, "page" | "turn_started")
            || (event_type == "user_message_chunk" && saw_tool)
        {
            scope += 1;
            saw_tool = false;
        }
        scopes.push(scope);
        let anchor = Anchor {
            index,
            session: meta(event, "session_id").into(),
            scope,
        };
        if event_type == "tool_call" {
            calls
                .entry(string(event, "tool_call_id").into())
                .or_default()
                .push(anchor.clone());
            if string(event, "name") == "Skill" {
                skill_calls
                    .entry(string(event, "tool_call_id").into())
                    .or_default()
                    .push(anchor.clone());
            }
        }
        if event_type == "system_message" {
            if string(event, "kind") == "shell_input" {
                shells
                    .entry(meta(event, "uuid").into())
                    .or_default()
                    .push(anchor.clone());
            }
            if string(event, "kind") == "command" && chip(string(event, "text")).is_some() {
                commands
                    .entry(meta(event, "uuid").into())
                    .or_default()
                    .push(anchor);
            }
        }
        if matches!(event_type, "tool_call" | "tool_call_update" | "tool_diff") {
            saw_tool = true;
        }
        if event_type == "turn_ended" {
            endings.insert(scope, string(event, "reason"));
            scope += 1;
            saw_tool = false;
        }
    }
    let mut updates = HashMap::<usize, Vec<usize>>::new();
    let mut diffs = HashMap::<usize, Vec<usize>>::new();
    let mut attached = HashMap::<usize, Vec<usize>>::new();
    let mut joined = HashSet::new();
    for (index, record) in records.iter().enumerate() {
        let event = &record.event;
        let scope = scopes[index];
        let kind = string(event, "kind");
        let target = match string(event, "type") {
            "tool_call_update" => anchor_match(string(event, "tool_call_id"), event, scope, &calls)
                .inspect(|&call| {
                    updates.entry(call).or_default().push(index);
                }),
            "tool_diff" => anchor_match(string(event, "tool_call_id"), event, scope, &calls)
                .inspect(|&call| {
                    diffs.entry(call).or_default().push(index);
                }),
            "system_message" => {
                let target = match kind {
                    "shell_output" => {
                        anchor_match(meta(event, "parent_uuid"), event, scope, &shells)
                            .filter(|target| !attached.contains_key(target))
                    }
                    "command_output" => {
                        anchor_match(meta(event, "parent_uuid"), event, scope, &commands)
                    }
                    "skill_body" | "injected"
                        if kind == "skill_body"
                            || !["<system-reminder>", "<local-command-caveat>"].iter().any(
                                |prefix| string(event, "text").trim_start().starts_with(prefix),
                            ) =>
                    {
                        anchor_match(meta(event, "parent_uuid"), event, scope, &commands).or_else(
                            || {
                                anchor_match(
                                    meta(event, "source_tool_use_id"),
                                    event,
                                    scope,
                                    if kind == "injected" {
                                        &skill_calls
                                    } else {
                                        &calls
                                    },
                                )
                            },
                        )
                    }
                    _ => None,
                };
                if let Some(target) = target {
                    attached.entry(target).or_default().push(index);
                }
                target
            }
            _ => None,
        };
        if target.is_some() {
            joined.insert(index);
        }
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut used = HashSet::new();
    let mut last_source_id = None;
    let mut prompt_expected = context.map_or(!has_older, |value| value.0);
    for (index, record) in records.iter().enumerate() {
        if joined.contains(&index) {
            last_source_id = None;
            continue;
        }
        let event = &record.event;
        if matches!(string(event, "type"), "turn_started" | "turn_ended") {
            prompt_expected = true;
        }
        let Some(mut row) = row(record) else {
            continue;
        };
        if is_codex
            && (row.kind == RowKind::User || row.system_kind.as_deref() == Some("swarm_ring"))
        {
            if row.kind == RowKind::User && !prompt_expected {
                row.arrives_mid_turn = true;
            }
            prompt_expected = false;
        }
        if string(event, "type") == "tool_call" {
            let last_update = updates
                .get(&index)
                .and_then(|updates| updates.last())
                .map(|index| &records[*index]);
            let final_status = last_update
                .map(|record| string(&record.event, "status"))
                .unwrap_or_else(|| string(event, "status"));
            let state = match final_status {
                "failed" => ToolState::Failed,
                "completed" if last_update.is_some() => ToolState::Finished,
                _ => match endings.get(&scopes[index]).copied() {
                    Some("aborted") => ToolState::Interrupted,
                    Some("completed") => ToolState::Unreported,
                    _ => {
                        if final_status == "completed" {
                            ToolState::Finished
                        } else {
                            ToolState::Waiting
                        }
                    }
                },
            };
            let input = event["input"].clone();
            let name = string(event, "name");
            let command = tool::command(&input, name);
            let path = tool::path(&input);
            let state = if is_codex
                && state == ToolState::Finished
                && command.is_some()
                && last_update.is_some_and(|record| record.output_failed)
            {
                ToolState::Failed
            } else {
                state
            };
            row.tool = Some(ToolActivity {
                name: name.into(),
                header_title: tool::title(&input, command.as_deref(), path.as_deref()),
                input,
                output: last_update.and_then(|record| record.text_preview.clone()),
                diffs: diffs
                    .get(&index)
                    .into_iter()
                    .flatten()
                    .filter_map(|index| records[*index].diff_preview.clone())
                    .collect(),
                state,
                exit_code: if command.is_some() {
                    last_update.and_then(|record| record.output_exit_code)
                } else {
                    None
                },
                command,
                path,
                duration_seconds: last_update.and_then(|record| {
                    tool::duration(meta(event, "timestamp"), meta(&record.event, "timestamp"))
                }),
                skill_body: None,
            });
        }
        for source in attached.get(&index).into_iter().flatten() {
            let child = &records[*source];
            let event = &child.event;
            let text = string(event, "text");
            match string(event, "kind") {
                "shell_output" => {
                    let mut run = child.shell.clone().unwrap();
                    run.command = row.shell.as_ref().and_then(|run| run.command.clone());
                    row = shell_row(run, row.id);
                }
                "skill_body" | "injected" => {
                    let body = if let Some(tool) = &mut row.tool {
                        &mut tool.skill_body
                    } else if let Some(command) = &mut row.command {
                        row.starts_turn = true;
                        &mut command.skill_body
                    } else {
                        continue;
                    };
                    if let Some(earlier) = body {
                        let combined =
                            TextPreview::new(&(earlier.text.clone() + "\n\n" + text), 120);
                        earlier.text = combined.text;
                        earlier.is_truncated |= combined.is_truncated
                            || child
                                .text_preview
                                .as_ref()
                                .is_some_and(|preview| preview.is_truncated);
                        earlier.full_bytes += 2 + child
                            .text_preview
                            .as_ref()
                            .map_or(text.len(), |preview| preview.full_bytes);
                        earlier.full_lines += 1 + child
                            .text_preview
                            .as_ref()
                            .map_or(1, |preview| preview.full_lines);
                        earlier.hidden_lines = earlier
                            .full_lines
                            .saturating_sub(TextPreview::new(&earlier.text, 120).full_lines);
                    } else {
                        *body = child.text_preview.clone();
                    }
                }
                _ => {
                    if let Some(command) = &mut row.command {
                        command.output = child.command_output.clone();
                    }
                }
            }
        }
        row.anchor_source_id = record.source_id.clone();
        let mut sources = vec![index];
        sources.extend(updates.get(&index).into_iter().flatten());
        sources.extend(diffs.get(&index).into_iter().flatten());
        sources.extend(attached.get(&index).into_iter().flatten());
        sources.sort_unstable();
        row.source_ids = sources
            .iter()
            .map(|index| records[*index].source_id.clone())
            .collect();
        if let Some(last) = rows.last_mut()
            && last.kind == row.kind
            && last_source_id.as_ref() == Some(&row.id)
            && matches!(
                row.kind,
                RowKind::User | RowKind::Assistant | RowKind::Thought
            )
        {
            last.text += &row.text;
            last.source_ids.extend(row.source_ids);
            continue;
        }
        last_source_id = Some(row.id.clone());
        if used.contains(&row.id) {
            row.id += &format!("#{}", record.source_id);
        }
        used.insert(row.id.clone());
        rows.push(row);
    }
    let first_boundary = rows.iter().find(|row| row.opens_turn() || row.ends_turn);
    let mut turn_open = context.map_or(
        has_older && first_boundary.is_none_or(|row| row.ends_turn),
        |value| value.1,
    );
    for row in &mut rows {
        if row.system_kind.as_deref() == Some("swarm_ring") && !turn_open {
            row.starts_turn = true;
        }
        if row.opens_turn() {
            turn_open = true;
        }
        if row.ends_turn {
            turn_open = false;
        }
        row.hidden = is_hidden(row);
    }
    rows
}

pub fn is_hidden(row: &Row) -> bool {
    (row.kind == RowKind::Notice
        && !row.starts_turn
        && ["hook_success", "title:", "model:"]
            .iter()
            .any(|prefix| row.text.starts_with(prefix)))
        || (row.kind == RowKind::System
            && (row.text.trim().is_empty() || row.system_kind.as_deref() == Some("injected")))
}
