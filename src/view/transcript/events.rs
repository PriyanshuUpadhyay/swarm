//! Transcript rows for individual events.

use super::super::tool;
use super::{
    Record, Row, RowKind, TextPreview, chip, is_record, key, meta, peer_text, record_text,
    shell_row, shell_run, string, tagged, task_summary,
};

pub fn row(record: &Record) -> Option<Row> {
    let event = &record.event;
    let event_type = string(event, "type");
    let text = string(event, "text");
    let mut row = match event_type {
        "user_message_chunk" | "agent_message_chunk" | "agent_thought_chunk" => {
            let kind = match event_type {
                "user_message_chunk" => RowKind::User,
                "agent_message_chunk" => RowKind::Assistant,
                _ => RowKind::Thought,
            };
            Row::new(kind, text.into(), meta(event, "uuid").into())
        }
        "tool_call" => {
            let command = tool::command(&event["input"], string(event, "name"));
            let path = tool::path(&event["input"]);
            let title = tool::title(&event["input"], command.as_deref(), path.as_deref());
            let name = string(event, "name");
            let mut row = Row::new(
                RowKind::ToolUse,
                if title.is_empty() {
                    name.into()
                } else {
                    format!("{name} · {title}")
                },
                format!("{}:call", string(event, "tool_call_id")),
            );
            row.detail = Some(event["input"].to_string());
            row.tool_status = Some(string(event, "status").into());
            row
        }
        "tool_call_update" => {
            let mut row = Row::new(
                RowKind::ToolResult,
                string(event, "content").into(),
                format!("{}:result", string(event, "tool_call_id")),
            );
            row.output = record.text_preview.clone();
            row.tool_status = Some(string(event, "status").into());
            row
        }
        "tool_diff" => {
            let Some(diff) = record.diff_preview.clone() else {
                return Some(Row::new(
                    RowKind::Notice,
                    event.to_string(),
                    key(record, "unknown"),
                ));
            };
            let mut row = Row::new(
                RowKind::Diff,
                format!(
                    "{} · +{} −{}",
                    tool::basename(&diff.path),
                    diff.added,
                    diff.removed
                ),
                format!("{}:diff", string(event, "tool_call_id")),
            );
            row.detail = Some(diff.path.clone());
            row.diff = Some(diff);
            row
        }
        "elicitation" | "elicitation_result" => {
            let questions = event_type == "elicitation";
            let field = if questions { "questions" } else { "answers" };
            let text_field = if questions { "question" } else { "answer" };
            let text = event[field]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| string(entry, text_field))
                .collect::<Vec<_>>()
                .join("\n");
            Row::new(
                if questions {
                    RowKind::Question
                } else {
                    RowKind::Result
                },
                text,
                format!(
                    "{}:{}",
                    string(event, "tool_call_id"),
                    if questions { "ask" } else { "answer" }
                ),
            )
        }
        "permission_decision" => Row::new(
            RowKind::Result,
            string(event, "decision").into(),
            format!("{}:decision", string(event, "tool_call_id")),
        ),
        "error" => Row::new(
            RowKind::Error,
            string(event, "message").into(),
            key(record, "error"),
        ),
        "system_message" => {
            let kind = string(event, "kind");
            match kind {
                "queued_prompt" => {
                    let mut row = Row::new(RowKind::User, text.into(), key(record, "queued"));
                    row.system_kind = Some(kind.into());
                    row.arrives_mid_turn = true;
                    row
                }
                _ if text.trim_start().starts_with("<task-notification>") => {
                    let mut row =
                        Row::new(RowKind::Notice, task_summary(text), key(record, "task"));
                    row.arrives_mid_turn = kind == "queued_command";
                    row.starts_turn = !row.arrives_mid_turn;
                    row
                }
                "peer_message" => {
                    let mut row = Row::new(RowKind::Notice, peer_text(text), key(record, "peer"));
                    row.starts_turn = true;
                    row
                }
                "shell_input" => shell_row(
                    shell_run(
                        tagged(text, "bash-input").map(|command| command.trim().into()),
                        "",
                        format!("{}:output", record.source_id),
                    ),
                    key(record, "shell"),
                ),
                "shell_output" => shell_row(record.shell.clone().unwrap(), key(record, "shell")),
                "interrupted" => {
                    let mut row = Row::new(
                        RowKind::Notice,
                        "Interrupted".into(),
                        key(record, "interrupted"),
                    );
                    row.ends_turn = true;
                    row
                }
                _ => {
                    let mut row = Row::new(
                        RowKind::System,
                        text.into(),
                        key(
                            record,
                            if kind == "swarm_ring" {
                                "ring"
                            } else {
                                "system"
                            },
                        ),
                    );
                    row.system_kind = Some(kind.into());
                    row.output = record.text_preview.clone();
                    if kind == "command" {
                        row.command = chip(text);
                    }
                    row
                }
            }
        }
        _ if is_record(event) => {
            let detail = match event_type {
                "queue_operation" => string(event, "operation"),
                _ => string(event, "kind"),
            };
            let mut row = Row::new(
                RowKind::System,
                [event_type, detail]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · "),
                // Its own id form: a uuid-less queue edit also makes a queued row under `key`.
                format!("record-{}", record.source_id),
            );
            row.system_kind = Some("record".into());
            row.output = record.text_preview.clone();
            row
        }
        "session_info" => Row::new(
            RowKind::Notice,
            format!("{}: {}", string(event, "kind"), string(event, "value")),
            key(record, "info"),
        ),
        "hook_result" => Row::new(
            RowKind::Notice,
            format!("{}: {}", string(event, "kind"), string(event, "hook_name")),
            key(record, "hook"),
        ),
        "turn_ended" => {
            let mut row = Row::new(
                RowKind::Result,
                string(event, "reason").into(),
                key(record, "result"),
            );
            row.ends_turn = true;
            row.detail = event["duration_ms"]
                .as_f64()
                .map(|duration| tool::duration_label(duration / 1000.0));
            row
        }
        "image" => Row::new(
            RowKind::Notice,
            format!(
                "{} image ({})",
                string(event, "role"),
                string(event, "media_type")
            ),
            key(record, "image"),
        ),
        // A page marker is the tool's own window bound, not a log record.
        "page" => return None,
        // A new event type shows as its JSON until the view learns it.
        _ => {
            let mut row = Row::new(
                RowKind::System,
                event_type.into(),
                format!("record-{}", record.source_id),
            );
            row.system_kind = Some("record".into());
            let mut preview = TextPreview::new(&record_text(event), 120);
            preview.id = format!("{}:output", record.source_id);
            row.output = Some(preview);
            row
        }
    };
    if row.id.is_empty() || row.id.starts_with(':') {
        row.id = format!("event-{}", record.source_id);
    }
    Some(row)
}
