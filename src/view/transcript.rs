use super::{DiffPreview, TextPreview, ToolActivity};
use super::{builder, fold_rows, queue};
use serde::Serialize;
use serde_json::Value;

pub(super) mod checklist;
pub mod display_comments;
mod display_patch;
mod events;
pub mod find_matches;
pub mod find_wire;
pub mod image_paths;
pub mod live_shown;
pub mod markdown;
mod normalize;
pub mod patch_wire;
pub mod queue_wire;
pub mod reader_rows;
mod records;
mod shell;
pub mod source;
pub mod source_follow;
mod state;
pub mod worker_protocol;

pub use checklist::TodoItem;
pub use events::row;
pub(super) use events::subagent_labels;
pub use records::Record;
pub(super) use shell::shell_row;
pub use shell::{clean_shell, command_output_text, shell_output_text, shell_run, strip_terminal};
pub use state::{
    ChatUsage, UsageState, clear_boundary, model_fact, queued, usage, view_rows,
    view_rows_with_context,
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
    #[serde(rename = "model_change")]
    ModelChange,
    Fold,
    Queued,
    TodoList,
    ProposedPlan,
    Subagent,
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
/// A subagent call: who it is, what it was asked, how far it got, and what it returned.
/// `call_id` names the child log to the server (ADR 0140).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SubagentRun {
    pub call_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub other_call_ids: Vec<String>,
    pub agent_type: String,
    pub description: String,
    pub state: super::ToolState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<TextPreview>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShellRun {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub output: TextPreview,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
}
/// `Row.system_kind` of a hidden bookkeeping record, such as an `ignored` event.
pub const RECORD: &str = "record";
/// `Row.system_kind` of a record no parser maps; the web counts these rows (ADR 0139).
pub const UNMAPPED: &str = "unmapped";

pub(crate) const CODEX_MIRROR: &str = "codex_mirror";
pub(crate) const CONTEXT_COMPACTED: &str = "Context compacted";

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent: Option<SubagentRun>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
    /// On a linked page, the older chair session that owns this row and its source IDs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_session: Option<String>,
    #[serde(skip)]
    pub anchor_source_id: String,
    /// The group that the agent's CLI draws this tool call in (`fold_rows`); empty when none.
    #[serde(skip)]
    pub group: String,
    /// What the call counts as in its group's label, such as "read" or "search".
    #[serde(skip)]
    pub group_part: String,
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
    /// The checklist of a `todoList` row.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub todo: Vec<TodoItem>,
    #[serde(skip_serializing_if = "super::is_false")]
    pub default_open: bool,
    pub order: [u64; 2],
}
impl RowKind {
    /// A to-do list or a plan: drawn as its own row, never as a tool step.
    pub fn is_checklist(self) -> bool {
        matches!(self, RowKind::TodoList | RowKind::ProposedPlan)
    }
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
            subagent: None,
            source_ids: Vec::new(),
            source_session: None,
            anchor_source_id: String::new(),
            group: String::new(),
            group_part: String::new(),
            ends_turn: false,
            starts_turn: false,
            arrives_mid_turn: false,
            collapsed: false,
            output: None,
            fold: None,
            steps: Vec::new(),
            todo: Vec::new(),
            default_open: false,
            order: [0, 0],
        }
    }
    pub fn scoped_id(session: &str, id: &str) -> String {
        format!("{session}:{id}")
    }

    fn scope_id(id: &mut String, session: &str) {
        if !id.is_empty() {
            *id = Self::scoped_id(session, id);
        }
    }

    pub fn scope_to(&mut self, session: &str) {
        self.source_session = Some(session.into());
        Self::scope_id(&mut self.id, session);

        if let Some(diff) = &mut self.diff {
            Self::scope_id(&mut diff.id, session);
        }
        let command = self.command.as_mut();
        for preview in [
            self.output.as_mut(),
            self.shell.as_mut().map(|shell| &mut shell.output),
            self.subagent.as_mut().and_then(|run| run.result.as_mut()),
        ]
        .into_iter()
        .flatten()
        {
            Self::scope_id(&mut preview.id, session);
        }
        if let Some(tool) = &mut self.tool {
            for diff in &mut tool.diffs {
                Self::scope_id(&mut diff.id, session);
            }
            for preview in [&mut tool.output, &mut tool.skill_body]
                .into_iter()
                .flatten()
            {
                Self::scope_id(&mut preview.id, session);
            }
        }
        if let Some(command) = command {
            for preview in [&mut command.output, &mut command.skill_body]
                .into_iter()
                .flatten()
            {
                Self::scope_id(&mut preview.id, session);
            }
        }
        for step in &mut self.steps {
            step.scope_to(session);
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
            RowKind::ModelChange => "Model changed".into(),
            RowKind::Fold => "Steps".into(),
            RowKind::Queued => "Queued".into(),
            RowKind::TodoList => "To-do".into(),
            RowKind::ProposedPlan => "Plan".into(),
            RowKind::Subagent => "Subagent".into(),
        }
    }
}
/// A log record for an ignored or unknown line, or bookkeeping that the view reads elsewhere.
/// The server keeps its collapsed row; the web skips it in the transcript and search (ADR 0099).
fn is_record(event: &Value) -> bool {
    matches!(
        string(event, "type"),
        "ignored" | "unknown" | "usage" | "queue_operation" | "turn_started" | "thread_rolled_back"
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
