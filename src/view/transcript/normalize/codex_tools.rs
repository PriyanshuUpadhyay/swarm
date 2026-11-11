//! Tool inputs keep scoped raw numbers; output actions defer question state to the reader.

use super::claude_input::{self, Input};
use super::claude_json::{self, Object};
use super::codex_facts::{Action, Fact, Template};
use super::codex_questions::{self, QuestionOutput};
use super::{SnapshotError, Work};
use serde_json::{Value, json, value::RawValue};

pub(super) fn append(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    kind: &str,
    work: &mut dyn Work,
) -> Result<bool, SnapshotError> {
    match kind {
        "web_search_call" | "tool_search_call" => {
            let web = kind == "web_search_call";
            let raw = payload.get(if web { "action" } else { "arguments" });
            let input = match raw {
                Some(raw) => claude_input::parse(raw, 255, work)?,
                None => Some(Input::Scalar(Value::Null)),
            };
            if let Some(input) = input {
                let mut event = Template::new(
                    "tool_call",
                    json!({
                        "tool_call_id": payload.text("call_id"),
                        "name": if web { "web_search" } else { "tool_search" },
                        "status": if web { "completed" } else { "pending" },
                    }),
                );
                event.input = Some(input);
                fact.actions.push(Action::Event(event));
            } else {
                fact.push_unknown();
            }
        }
        "tool_search_output" => {
            let input = match payload.get("tools") {
                Some(raw) => claude_input::parse(raw, 255, work)?,
                None => Some(Input::Scalar(Value::Null)),
            };
            if let Some(input) = input {
                let mut content = String::new();
                if !matches!(&input, Input::Scalar(Value::Null)) {
                    write_tool_list(&input, &mut content, work)?;
                }
                fact.push("tool_call_update", json!({
                    "tool_call_id": payload.text("call_id"), "status": "completed", "content": content,
                }));
            } else {
                fact.push_unknown();
            }
        }
        "function_call" | "custom_tool_call" => {
            let (Some(call_id), Some(name), Some(text)) = (
                payload.optional_text("call_id"),
                payload.optional_text("name"),
                payload.optional_text(if kind == "function_call" {
                    "arguments"
                } else {
                    "input"
                }),
            ) else {
                fact.push_unknown();
                return Ok(true);
            };
            let raw = serde_json::from_str::<&RawValue>(&text).ok();
            let raw = match raw {
                Some(raw) if claude_json::valid(raw, work)? => Some(raw),
                _ => None,
            };
            let input = match raw {
                Some(raw) => claude_input::parse(raw, 255, work)?,
                None => Some(Input::Scalar(Value::String(text.clone()))),
            };
            let Some(input) = input else {
                fact.push_unknown();
                return Ok(true);
            };
            if matches!(
                name.as_str(),
                "request_user_input" | "request_user_input_async"
            ) {
                let (questions, ids) = match raw {
                    Some(raw) => codex_questions::questions(raw, work)?,
                    None => (json!([]), vec![String::new()]),
                };
                let event = Template::new(
                    "elicitation",
                    json!({"tool_call_id": call_id, "questions": questions}),
                );
                fact.actions.push(Action::Open {
                    call_id,
                    ids,
                    event,
                });
            } else {
                let mut event = Template::new(
                    "tool_call",
                    json!({
                        "tool_call_id": call_id, "name": name, "status": "pending",
                    }),
                );
                event.input = Some(input);
                fact.actions.push(Action::Event(event));
            }
        }
        "function_call_output" | "custom_tool_call_output" => {
            output(fact, payload, kind == "function_call_output", work)?
        }
        _ => return Ok(false),
    }
    Ok(true)
}

// The tool list is JSON inside a string, so OLD's positional float spelling is observable.
fn write_tool_list(
    input: &Input,
    content: &mut String,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    work.check()?;
    match input {
        Input::Scalar(_) => {
            content.push_str(&serde_json::to_string(input).expect("validated tool list scalar"));
        }
        Input::Number(raw) => content.push_str(raw.get()),
        Input::Array(items) => {
            content.push('[');
            for (index, item) in items.iter().enumerate() {
                if index != 0 {
                    content.push(',');
                }
                write_tool_list(item, content, work)?;
            }
            content.push(']');
        }
        Input::Object(fields) => {
            content.push('{');
            for (index, (key, value)) in fields.iter().enumerate() {
                if index != 0 {
                    content.push(',');
                }
                content.push_str(&serde_json::to_string(key).expect("valid tool list key"));
                content.push(':');
                write_tool_list(value, content, work)?;
            }
            content.push('}');
        }
    }
    Ok(())
}

fn output(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    is_function: bool,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let (Some(call_id), Some(raw)) = (payload.optional_text("call_id"), payload.get("output"))
    else {
        fact.push_unknown();
        return Ok(());
    };
    let mut tail = Vec::new();
    let (content, question) = if let Ok(text) = serde_json::from_str::<String>(raw.get()) {
        let question = if is_function {
            codex_questions::output(&text, work)?
        } else {
            QuestionOutput::None
        };
        (text, question)
    } else if let Some(parts) = claude_json::array(raw) {
        let mut text = String::new();
        let mut has_unknown = false;
        for raw in parts {
            work.check()?;
            if let Some(part) = Object::parse(raw) {
                let kind = part.text("type");
                if kind == "input_image" {
                    tail.push(Template::new(
                        "image",
                        json!({"role": "tool", "media_type": codex_questions::media_type(&part)}),
                    ));
                    continue;
                }
                if matches!(kind.as_str(), "text" | "input_text")
                    && let Some(chunk) = part.optional_text("text")
                {
                    text.push_str(&chunk);
                    continue;
                }
            }
            has_unknown = true;
        }
        if has_unknown {
            tail.push(fact.unknown());
        }
        (text, QuestionOutput::None)
    } else {
        fact.push_unknown();
        return Ok(());
    };
    let mut fields = json!({
        "tool_call_id": call_id, "status": "completed", "content": content,
    });
    if let Some(agent_id) = serde_json::from_str::<Value>(&content)
        .ok()
        .and_then(|output| output["agent_id"].as_str().map(str::to_owned))
    {
        fields["agent_thread_id"] = json!(agent_id);
    }
    let event = Template::new("tool_call_update", fields);
    fact.actions.push(Action::Output {
        call_id,
        question,
        event,
        tail,
    });
    Ok(())
}
