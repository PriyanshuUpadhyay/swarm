use super::tool;
use super::{Diff, DiffPreview, TextPreview, ToolActivity};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub source_id: String,
    pub event: Value,
    pub text_preview: Option<TextPreview>,
    pub diff_preview: Option<DiffPreview>,
    pub output_exit_code: Option<i64>,
    pub output_failed: bool,
    pub shell: Option<ShellRun>,
    pub command_output: Option<TextPreview>,
}
impl Record {
    /// Drops full tool text after preview and failure extraction. The log owns the full copy.
    pub fn new(mut event: Value, source_id: String, is_codex: bool) -> Self {
        let shell = if string(&event, "type") == "system_message"
            && string(&event, "kind") == "shell_output"
        {
            Some(shell_run(
                None,
                string(&event, "text"),
                format!("{source_id}:output"),
            ))
        } else {
            None
        };
        let command_output = if string(&event, "type") == "system_message"
            && string(&event, "kind") == "command_output"
        {
            let mut preview = TextPreview::new(&command_output_text(string(&event, "text")), 120);
            preview.id = format!("{source_id}:output");
            Some(preview)
        } else {
            None
        };
        let mut text_preview = None;
        let mut diff_preview = None;
        let mut output_exit_code = None;
        let mut output_failed = false;
        let field = match string(&event, "type") {
            "tool_call_update" => Some("content"),
            "system_message" => Some("text"),
            _ => None,
        };
        if let Some(field) = field {
            let text = string(&event, field);
            if field == "content" {
                output_exit_code = tool::exit_code(Some(text), Some(""), is_codex);
                output_failed = tool::reports_failure(Some(text), Some(""), is_codex);
            }
            let mut preview = TextPreview::new(text, 120);
            preview.id = format!("{source_id}:output");
            if let Some(object) = event.as_object_mut() {
                object.insert(field.into(), Value::String(preview.text.clone()));
            }
            text_preview = Some(preview);
        }
        if string(&event, "type") == "tool_diff"
            && let Ok(diff) = serde_json::from_value::<Diff>(event.clone())
        {
            let mut preview = DiffPreview::new(&diff);
            preview.id = format!("{source_id}:diff");
            // Keep metadata, but never keep the full patch in shared follower state.
            if let Some(object) = event.as_object_mut() {
                object.remove("hunks");
            }
            diff_preview = Some(preview);
        }
        Self {
            source_id,
            event,
            text_preview,
            diff_preview,
            output_exit_code,
            output_failed,
            shell,
            command_output,
        }
    }
}

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
    pub hidden: bool,
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
            hidden: false,
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

pub fn clean_shell(text: &str) -> String {
    fn decode(text: &str) -> String {
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }
    let mut decoded = String::new();
    let mut rest = text;
    while let Some((before, body)) = rest.split_once("<persisted-output>") {
        decoded += &decode(before);
        decoded += "<persisted-output>";
        let end = body
            .find("</persisted-output>")
            .map(|index| index + "</persisted-output>".len())
            .unwrap_or(body.len());
        decoded += &body[..end];
        rest = &body[end..];
    }
    decoded += &decode(rest);
    let normalized = decoded.replace("\r\n", "\n");
    let mut characters = normalized.chars().peekable();
    let mut output = String::new();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            match characters.next() {
                Some('[') => {
                    while characters
                        .peek()
                        .is_some_and(|character| (' '..='?').contains(character))
                    {
                        characters.next();
                    }
                    if characters
                        .peek()
                        .is_some_and(|character| ('@'..='~').contains(character))
                    {
                        characters.next();
                    }
                }
                Some(']') => {
                    for character in characters.by_ref() {
                        if character == '\u{07}' || character == '\u{1b}' {
                            break;
                        }
                    }
                    if characters.peek() == Some(&'\\') {
                        characters.next();
                    }
                }
                _ => (),
            }
        } else if !character.is_control() || matches!(character, '\n' | '\t') {
            output.push(character);
        }
    }
    output
}
pub fn command_output_text(text: &str) -> String {
    let parts: Vec<_> = ["local-command-stdout", "local-command-stderr"]
        .into_iter()
        .filter_map(|tag| tagged(text, tag))
        .collect();
    clean_shell(&if parts.is_empty() {
        text.into()
    } else {
        parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    })
}
pub fn shell_run(command: Option<String>, text: &str, output_id: String) -> ShellRun {
    let output_text = shell_output_text(text);
    let mut output = TextPreview::new(&output_text, 120);
    output.id = output_id;
    ShellRun {
        command,
        output,
        exit_code: tagged(text, "bash-exit-code").and_then(|code| code.trim().parse().ok()),
    }
}
pub fn shell_output_text(text: &str) -> String {
    let stdout = clean_shell(tagged(text, "bash-stdout").unwrap_or(""));
    let stderr = clean_shell(tagged(text, "bash-stderr").unwrap_or(""));
    let stdout = if stdout == "(Bash completed with no output)" {
        ""
    } else {
        stdout.trim_end_matches(['\n', '\r'])
    };
    [stdout, stderr.trim_end_matches(['\n', '\r'])]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
pub(super) fn shell_row(run: ShellRun, id: String) -> Row {
    let text = [
        run.command.as_ref().map(|command| format!("$ {command}")),
        (!run.output.text.is_empty()).then(|| run.output.text.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n");
    let mut row = Row::new(RowKind::Shell, text, id);
    row.shell = Some(run);
    row.starts_turn = true;
    row
}

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
        "session_info" if string(event, "kind") == "agent_name" => return None,
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
        "unknown" => Row::new(
            RowKind::Notice,
            string(event, "raw").into(),
            key(record, "unknown"),
        ),
        _ => return None,
    };
    if row.id.is_empty() || row.id.starts_with(':') {
        row.id = format!("event-{}", record.source_id);
    }
    Some(row)
}

pub use super::builder::{is_hidden, rows};

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
