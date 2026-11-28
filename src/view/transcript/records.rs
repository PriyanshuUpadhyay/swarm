//! Log records and their bounded previews.

use super::super::{Diff, DiffPreview, PREVIEW_TAIL_LINES, last_lines, tool};
use super::{
    ShellRun, TextPreview, command_output_text, is_record, record_text, shell_run, string,
    strip_terminal,
};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub source_id: String,
    pub event: Value,
    pub(super) prior_model: Option<String>,
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
            // Every agent CLI drops terminal colour codes from tool output before it shows it.
            let clean = (field == "content").then(|| strip_terminal(string(&event, field)));
            let text = clean.as_deref().unwrap_or_else(|| string(&event, field));
            if field == "content" {
                output_exit_code = event["exit_code"]
                    .as_i64()
                    .filter(|code| *code != 0)
                    .or_else(|| tool::exit_code(Some(text), Some(""), is_codex));
                output_failed = tool::reports_failure(Some(text), Some(""), is_codex);
            }
            let mut preview = TextPreview::new(text, 120);
            preview.id = format!("{source_id}:output");
            if field == "content" && preview.is_truncated {
                preview.tail = last_lines(text, PREVIEW_TAIL_LINES);
            }
            if let Some(object) = event.as_object_mut() {
                object.insert(field.into(), Value::String(preview.text.clone()));
            }
            text_preview = Some(preview);
        }
        if is_record(&event) {
            let mut preview = TextPreview::new(&record_text(&event), 120);
            preview.id = format!("{source_id}:output");
            // The preview holds the record; the full-output route reads the line again.
            if let Some(object) = event.as_object_mut() {
                object.remove("raw");
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
            prior_model: None,
            text_preview,
            diff_preview,
            output_exit_code,
            output_failed,
            shell,
            command_output,
        }
    }
}
