use super::{DiffPreview, TextPreview, ToolActivity};
use super::{builder, fold_rows, queue};
use serde::Serialize;
use serde_json::Value;

mod events;
mod records;
mod shell;
mod state;

pub use events::row;
pub use records::Record;
pub(super) use shell::shell_row;
pub use shell::{clean_shell, command_output_text, shell_output_text, shell_run};
pub use state::{
    ChatUsage, UsageState, clear_boundary, queued, usage, view_rows, view_rows_with_context,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RowKind {
    User,
    Assistant,
    Thought,
    ToolUse,
    ToolResult,
    Diff,
    Question,
    Error,
    Notice,
    System,
    Result,
    Shell,
    Divider,
    Fold,
    Queued,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CommandChip {
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_body: Option<TextPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<TextPreview>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShellRun {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub output: TextPreview,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
}
/// Absent fields have the same meaning as null, false, or an empty array.
/// `id`, `kind`, `text`, and `order` are always present.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Row {
    pub id: String,
    pub kind: RowKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolActivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandChip>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
    #[serde(skip)]
    pub anchor_source_id: String,
    #[serde(skip_serializing_if = "super::is_false")]
    pub ends_turn: bool,
    #[serde(skip_serializing_if = "super::is_false")]
    pub starts_turn: bool,
    #[serde(skip_serializing_if = "super::is_false")]
    pub arrives_mid_turn: bool,
    #[serde(skip_serializing_if = "super::is_false")]
    pub collapsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<TextPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fold: Option<super::FoldSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Row>,
    #[serde(skip_serializing_if = "super::is_false")]
    pub default_open: bool,
    pub order: [u64; 2],
}
impl Row {
    pub fn new(kind: RowKind, text: String, id: String) -> Self {
        Self {
            id,
            kind,
            text,
            detail: None,
            diff: None,
            tool: None,
            tool_status: None,
            system_kind: None,
            shell: None,
            command: None,
            source_ids: Vec::new(),
            anchor_source_id: String::new(),
            ends_turn: false,
            starts_turn: false,
            arrives_mid_turn: false,
            collapsed: false,
            output: None,
            fold: None,
            steps: Vec::new(),
            default_open: false,
            order: [0, 0],
        }
    }
    pub fn opens_turn(&self) -> bool {
        !self.arrives_mid_turn && (self.kind == RowKind::User || self.starts_turn)
    }
    pub fn label(&self, chair: Option<&str>) -> String {
        match self.kind {
            RowKind::User => "You".into(),
            RowKind::Assistant => chair
                .map(|name| {
                    let mut characters = name.chars();
                    characters
                        .next()
                        .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
                        .unwrap_or_default()
                })
                .unwrap_or_else(|| "Chair".into()),
            RowKind::Thought => "Thinking".into(),
            RowKind::ToolUse => "Tool".into(),
            RowKind::ToolResult => "Result".into(),
            RowKind::Diff => "Changes".into(),
            RowKind::Question => "Question".into(),
            RowKind::Error => "Error".into(),
            RowKind::Notice => "Notice".into(),
            RowKind::System => "System".into(),
            RowKind::Result => "Turn".into(),
            RowKind::Shell => "Shell".into(),
            RowKind::Divider => "Context cleared".into(),
            RowKind::Fold => "Steps".into(),
            RowKind::Queued => "Queued".into(),
        }
    }
    pub fn search_text(&self) -> String {
        if self.system_kind.as_deref() == Some("swarm_ring") {
            return "New swarm message".into();
        }
        let mut parts = vec![self.text.as_str()];
        if let Some(detail) = &self.detail {
            parts.push(detail);
        }
        if let Some(tool) = &self.tool {
            if let Some(command) = &tool.command {
                parts.push(command);
            }
            if let Some(output) = &tool.output {
                parts.push(&output.text);
            }
            if let Some(path) = &tool.path {
                parts.push(path);
            }
            for diff in &tool.diffs {
                parts.push(&diff.path);
                parts.push(&diff.patch);
            }
            if let Some(body) = &tool.skill_body {
                parts.push(&body.text);
            }
        }
        if let Some(command) = &self.command {
            if let Some(output) = &command.output {
                parts.push(&output.text);
            }
            if let Some(body) = &command.skill_body {
                parts.push(&body.text);
            }
        }
        parts.join("\n")
    }
}
/// A log record with no row of its own: an ignored or unknown line, or bookkeeping that the view
/// reads elsewhere. It shows as a collapsed row that opens to the whole record.
fn is_record(event: &Value) -> bool {
    matches!(
        string(event, "type"),
        "ignored" | "unknown" | "usage" | "queue_operation" | "turn_started"
    ) || (string(event, "type") == "session_info" && string(event, "kind") == "agent_name")
}
/// The whole record: the source line of an ignored or unknown event, else the event itself.
/// JSON is pretty-printed, so the record reads as lines.
pub fn record_text(event: &Value) -> String {
    let raw = string(event, "raw");
    if raw.is_empty() {
        return serde_json::to_string_pretty(event).unwrap_or_default();
    }
    serde_json::from_str::<Value>(raw)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| raw.into())
}
/// The log formats the transcript tool reads, by provider id.
pub const LOG_FORMATS: [&str; 4] = ["claude", "codex", "agy", "pi"];

pub fn is_active(rows: &[Row]) -> bool {
    rows.iter()
        .rposition(|row| {
            row.opens_turn()
                || row.arrives_mid_turn
                || row.system_kind.as_deref() == Some("swarm_ring")
        })
        .is_some_and(|start| !rows[start..].iter().any(|row| row.ends_turn))
}
pub fn string<'a>(event: &'a Value, field: &str) -> &'a str {
    event[field].as_str().unwrap_or("")
}
pub(super) fn meta<'a>(event: &'a Value, field: &str) -> &'a str {
    event["meta"][field].as_str().unwrap_or("")
}
fn key(record: &Record, kind: &str) -> String {
    let uuid = meta(&record.event, "uuid");
    if uuid.is_empty() {
        format!("event-{}", record.source_id)
    } else {
        format!("{uuid}:{kind}")
    }
}
pub(super) fn chip(text: &str) -> Option<CommandChip> {
    let name = tagged(text, "command-name").unwrap_or("").trim();
    (!name.is_empty()).then(|| CommandChip {
        name: name.into(),
        arguments: tagged(text, "command-args").unwrap_or("").trim().into(),
        skill_body: None,
        output: None,
    })
}
pub fn tagged<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(&format!("<{tag}>"))?;
    rest.split_once(&format!("</{tag}>")).map(|(body, _)| body)
}
fn task_summary(text: &str) -> String {
    for tag in ["summary", "status"] {
        if let Some(value) = tagged(text, tag)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return if tag == "summary" {
                value.into()
            } else {
                format!("Background task {value}")
            };
        }
    }
    "Background task finished".into()
}
fn peer_text(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with('<') {
        return text.into();
    }
    let Some((head, body)) = trimmed[1..].split_once('>') else {
        return text.into();
    };
    let name = head.split_whitespace().next().unwrap_or("");
    body.strip_suffix(&format!("</{name}>"))
        .map(|body| body.trim().into())
        .unwrap_or_else(|| text.into())
}

pub use super::builder::{is_collapsed, rows};
