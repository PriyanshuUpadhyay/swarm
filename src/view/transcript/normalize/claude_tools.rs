//! Claude tool payloads and saved structured patches, with the provider's validation.

use super::claude_input::{self, Input};
use super::claude_json::{self, Object};
use super::claude_meta::{event, unknown};
use super::{NormalizedEvent, SnapshotError, Work};
use serde::Serialize;
use serde_json::{Value, json};

fn add_unknown(
    line: &[u8],
    meta: &Value,
    events: &mut Vec<NormalizedEvent>,
    has_unknown: &mut bool,
) {
    if !*has_unknown {
        events.push(unknown(line, meta));
        *has_unknown = true;
    }
}

pub(super) fn call(
    block: &Object<'_>,
    meta: &Value,
    line: &[u8],
    events: &mut Vec<NormalizedEvent>,
    has_unknown: &mut bool,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let input = match block.get("input") {
        Some(raw) => match claude_input::parse(raw, 255, work)? {
            Some(input) => input,
            None => {
                add_unknown(line, meta, events, has_unknown);
                return Ok(());
            }
        },
        None => Input::Scalar(Value::Null),
    };
    let name = block.text("name");
    let id = block.text("id");
    if name == "AskUserQuestion" {
        events.push(event(
            "elicitation",
            meta,
            json!({"tool_call_id":id, "questions":questions(block.get("input"), work)?}),
        ));
    } else {
        #[derive(Serialize)]
        struct Call<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            tool_call_id: String,
            name: String,
            input: Input,
            status: &'static str,
            meta: &'a Value,
        }
        events.push(NormalizedEvent::new(&Call {
            kind: "tool_call",
            tool_call_id: id,
            name,
            input,
            status: "pending",
            meta,
        }));
    }
    Ok(())
}

fn questions(
    input: Option<&serde_json::value::RawValue>,
    work: &mut dyn Work,
) -> Result<Vec<Value>, SnapshotError> {
    let Some(items) = input
        .and_then(Object::parse)
        .and_then(|obj| obj.array("questions"))
    else {
        return Ok(Vec::new());
    };
    let mut questions = Vec::new();
    for item in items {
        work.check()?;
        let Some(item) = Object::parse(item) else {
            continue;
        };
        let mut options = Vec::new();
        if let Some(items) = item.array("options") {
            for option in items {
                work.check()?;
                if let Some(option) = Object::parse(option) {
                    options.push(json!({
                        "label": option.text("label"),
                        "description": option.text("description"),
                    }));
                }
            }
        }
        questions.push(
            json!({"question":item.text("question"), "header":item.text("header"),
            "multi_select":item.boolean("multiSelect").unwrap_or(false), "options":options}),
        );
    }
    Ok(questions)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keep provider block context explicit at the tool-result boundary."
)]
pub(super) fn result(
    block: &Object<'_>,
    rec: &Object<'_>,
    meta: &Value,
    line: &[u8],
    result_count: usize,
    events: &mut Vec<NormalizedEvent>,
    has_unknown: &mut bool,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let result = rec.object("toolUseResult");
    if let Some(answers) = result.as_ref().and_then(|result| result.object("answers")) {
        let mut values = Vec::new();
        for (question, raw) in answers.fields() {
            work.check()?;
            if let Ok(answer) = serde_json::from_str::<String>(raw.get()) {
                values.push(json!({"question":question, "answer":answer}));
            }
        }
        events.push(event(
            "elicitation_result",
            meta,
            json!({"tool_call_id":block.text("tool_use_id"), "answers":values}),
        ));
        return Ok(());
    }
    let mut text = block.optional_text("content").unwrap_or_default();
    if let Some(parts) = block.array("content") {
        for part in parts {
            work.check()?;
            if let Some(part) = Object::parse(part)
                && part.text("type") == "text"
            {
                text.push_str(&part.text("text"));
            }
        }
    }
    let failed = block.boolean("is_error") == Some(true);
    let id = block.text("tool_use_id");
    let task_id = result
        .as_ref()
        .and_then(|result| result.object("task"))
        .and_then(|task| task.optional_text("id"));
    let mut fields = json!({"tool_call_id":id, "status":if failed {"failed"} else {"completed"}, "content":text});
    if let Some(task_id) = task_id {
        fields["task_id"] = json!(task_id);
    }
    events.push(event("tool_call_update", meta, fields));
    if !failed && let Some(result) = result {
        match patch(&result, &id, result_count, work)? {
            Ok(Some((path, hunks))) => events.push(event(
                "tool_diff",
                meta,
                json!({"tool_call_id":id, "path":path, "hunks":hunks}),
            )),
            Err(()) => add_unknown(line, meta, events, has_unknown),
            Ok(None) => (),
        }
    }
    Ok(())
}

type Patch = Result<Option<(String, Vec<Value>)>, ()>;

// This reads the saved patch. It never computes a diff or changes the Rust diff/view owners.
fn patch(
    result: &Object<'_>,
    id: &str,
    result_count: usize,
    work: &mut dyn Work,
) -> Result<Patch, SnapshotError> {
    let Some(raw) = result.get("structuredPatch") else {
        return Ok(Ok(None));
    };
    if raw.get() == "null" {
        return Ok(Ok(None));
    }
    let Some(items) = claude_json::array(raw) else {
        return Ok(Err(()));
    };
    if items.is_empty() {
        return Ok(Ok(None));
    }
    let path = result.text("filePath");
    if result_count != 1 || id.is_empty() || path.is_empty() {
        return Ok(Err(()));
    }
    let mut hunks = Vec::new();
    for raw in items {
        work.check()?;
        let Some(obj) = Object::parse(raw) else {
            return Ok(Err(()));
        };
        let Some(lines) = obj.array("lines") else {
            return Ok(Err(()));
        };
        let mut text = Vec::new();
        let mut old_count = 0i64;
        let mut new_count = 0i64;
        for line in lines {
            work.check()?;
            let Ok(line) = serde_json::from_str::<String>(line.get()) else {
                return Ok(Err(()));
            };
            if line.is_empty() || line.contains(['\r', '\n']) {
                return Ok(Err(()));
            }
            match line.as_bytes()[0] {
                b' ' => {
                    old_count += 1;
                    new_count += 1;
                }
                b'-' => old_count += 1,
                b'+' => new_count += 1,
                b'\\' if line == "\\ No newline at end of file" => (),
                _ => return Ok(Err(())),
            }
            text.push(line);
        }
        let (Some(old_start), Some(old_lines), Some(new_start), Some(new_lines)) = (
            obj.token("oldStart"),
            obj.token("oldLines"),
            obj.token("newStart"),
            obj.token("newLines"),
        ) else {
            return Ok(Err(()));
        };
        if old_count != old_lines || new_count != new_lines {
            return Ok(Err(()));
        }
        hunks.push(
            serde_json::to_value(crate::view::DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: text,
            })
            .expect("diff hunk is valid JSON"),
        );
    }
    Ok(Ok(Some((path, hunks))))
}
