//! Usage, queue, and transcript view state.

use super::{Record, Row, RowKind, key, meta, string};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChatUsage {
    pub context: Option<Value>,
    pub context_timestamp: String,
    pub context_notice: String,
    pub remaining_percent: Option<i64>,
}
#[derive(Clone, Default)]
pub struct UsageState {
    context: Option<Value>,
    model: Option<String>,
    reported: bool,
}
impl UsageState {
    pub fn ingest(&mut self, event: &Value) {
        match (string(event, "type"), string(event, "kind")) {
            ("usage", "context") => {
                self.context = Some(event.clone());
                self.reported = true;
            }
            ("system_message", "compaction") => self.context = None,
            ("session_info", "model") => {
                let model = string(event, "value");
                if self.model.as_ref().is_some_and(|old| old != model) {
                    self.context = None;
                }
                self.model = Some(model.into());
            }
            _ => (),
        }
    }
    pub fn has_report(&self) -> bool {
        self.reported
    }
    pub fn value(&self) -> Option<ChatUsage> {
        let context = self.context.as_ref()?;
        let remaining_percent = context["context_tokens"]
            .as_i64()
            .zip(context["context_capacity_tokens"].as_i64())
            .and_then(|(used, capacity)| {
                (capacity > 0).then(|| {
                    ((capacity.saturating_sub(used).max(0) as f64 / capacity as f64) * 100.0)
                        .round() as i64
                })
            });
        Some(ChatUsage {
            context: Some(context.clone()),
            context_timestamp: meta(context, "timestamp").into(),
            context_notice: if context["context_tokens"].is_null() {
                "Context tokens were not reported.".into()
            } else {
                String::new()
            },
            remaining_percent,
        })
    }
}
pub fn usage(records: &[Record]) -> Option<ChatUsage> {
    let mut state = UsageState::default();
    for record in records {
        state.ingest(&record.event);
    }
    state.value()
}
pub fn queued(records: &[Record]) -> Vec<Row> {
    let mut queue: Vec<(&Record, &str)> = Vec::new();
    for record in records {
        let event = &record.event;
        if string(event, "type") != "queue_operation" {
            continue;
        }
        match string(event, "operation") {
            "enqueue" => {
                if let Some(text) = event["content"].as_str() {
                    queue.push((record, text));
                }
            }
            "dequeue" => {
                if !queue.is_empty() {
                    queue.remove(0);
                }
            }
            "remove" | "popAll" => {
                if let Some(index) = queue
                    .iter()
                    .position(|(_, text)| *text == string(event, "content"))
                {
                    queue.remove(index);
                }
            }
            _ => (),
        }
    }
    queue
        .into_iter()
        .filter(|(_, text)| super::queue::is_owner(text))
        .map(|(record, text)| {
            let mut row = Row::new(RowKind::Queued, text.into(), key(record, "queued"));
            row.source_ids.push(record.source_id.clone());
            row
        })
        .collect()
}
pub fn clear_boundary(records: &[Record], rows: &mut Vec<Row>, at_log_start: bool) {
    if !at_log_start
        || !records.iter().take(30).any(|record| {
            let event = &record.event;
            (string(event, "type") == "hook_result"
                && string(event, "hook_name") == "SessionStart:clear")
                || (string(event, "type") == "system_message"
                    && string(event, "kind") == "command"
                    && string(event, "text").contains("<command-name>/clear</command-name>"))
        })
    {
        return;
    }
    let first_prose = rows
        .iter()
        .position(|row| matches!(row.kind, RowKind::User | RowKind::Assistant))
        .unwrap_or(rows.len());
    let mut index = 0;
    rows.retain(|row| {
        let before = index < first_prose;
        index += 1;
        !(before
            && row.kind == RowKind::System
            && (row.text.contains("<command-name>/clear</command-name>")
                || row.text.trim().starts_with("<local-command-caveat>")
                || row.text.trim() == "<local-command-stdout></local-command-stdout>"))
    });
    let mut divider = Row::new(
        RowKind::Divider,
        "Context cleared".into(),
        "context-cleared".into(),
    );
    divider.detail = records
        .first()
        .map(|record| meta(&record.event, "timestamp").into());
    rows.insert(0, divider);
}
pub fn view_rows(
    records: &[Record],
    has_older: bool,
    is_codex: bool,
    shown: &HashSet<String>,
) -> Vec<Row> {
    view_rows_with_context(records, has_older, is_codex, shown, None)
}
/// Context is (prompt_expected, turn_open) immediately before this record slice.
pub fn view_rows_with_context(
    records: &[Record],
    has_older: bool,
    is_codex: bool,
    shown: &HashSet<String>,
    context: Option<(bool, bool)>,
) -> Vec<Row> {
    let mut result = super::builder::rows_with_context(records, has_older, is_codex, context);
    clear_boundary(records, &mut result, !has_older && context.is_none());
    result.extend(queued(records));
    for row in &mut result {
        let source = if row.anchor_source_id.is_empty() {
            row.source_ids.first()
        } else {
            Some(&row.anchor_source_id)
        };
        if let Some(source) = source {
            row.id = format!("{}@{source}", row.id.split('#').next().unwrap_or(&row.id));
            let mut pieces = source.trim_start_matches("raw-").split('-');
            row.order = [
                pieces
                    .next()
                    .and_then(|piece| piece.parse().ok())
                    .unwrap_or(0),
                pieces
                    .next()
                    .and_then(|piece| piece.parse().ok())
                    .unwrap_or(0),
            ];
        }
    }
    super::fold_rows(result, shown)
}
