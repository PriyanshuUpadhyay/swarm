//! Native Codex question payloads retain source order until a reader knows the call order.

use super::claude_json::{self, Object};
use super::{SnapshotError, Work};
use serde::Serialize;
use serde_json::{Value, json, value::RawValue};

#[derive(Clone, Debug, Serialize)]
pub(super) struct Answer {
    pub(super) question: String,
    answer: String,
}

#[derive(Debug)]
pub(super) enum QuestionOutput {
    None,
    Accepted,
    Answers(Vec<Answer>),
}

pub(super) fn media_type(part: &Object<'_>) -> String {
    let media = part.text("media_type");
    if !media.is_empty() {
        return media;
    }
    let url = part.text("image_url");
    let Some(rest) = url.strip_prefix("data:") else {
        return String::new();
    };
    rest.find([';', ','])
        .map(|end| rest[..end].to_owned())
        .unwrap_or_default()
}

pub(super) fn questions(
    raw: &RawValue,
    work: &mut dyn Work,
) -> Result<(Value, Vec<String>), SnapshotError> {
    let items = Object::parse(raw)
        .and_then(|obj| obj.array("questions"))
        .unwrap_or_default();
    let mut questions = Vec::new();
    let mut ids = Vec::new();
    for raw in items {
        work.check()?;
        let Some(item) = Object::parse(raw) else {
            ids.push(String::new());
            continue;
        };
        ids.push(item.text("id"));
        let mut options = Vec::new();
        for raw in item.array("options").unwrap_or_default() {
            work.check()?;
            if let Ok(label) = serde_json::from_str::<String>(raw.get()) {
                options.push(json!({"label": label, "description": ""}));
            } else if let Some(option) = Object::parse(raw) {
                options.push(json!({"label": option.text("label"), "description": option.text("description")}));
            }
        }
        let text = item.text("question");
        questions.push(json!({
            "question": if text.is_empty() { item.text("title") } else { text },
            "header": item.text("header"), "multi_select": false, "options": options,
        }));
    }
    // OLD stores newline-joined IDs, including an empty entry for an empty list.
    let ids = ids.join("\n").split('\n').map(str::to_owned).collect();
    Ok((questions.into(), ids))
}

pub(super) fn reply(
    text: &str,
    work: &mut dyn Work,
) -> Result<Option<(String, Vec<Answer>)>, SnapshotError> {
    let text = text.trim_end_matches([' ', '\t', '\r', '\n']);
    let Some(body) = text
        .strip_prefix("<send_user_message_question_reply>")
        .and_then(|text| text.strip_suffix("</send_user_message_question_reply>"))
    else {
        return Ok(None);
    };
    let Ok(raw) = serde_json::from_str::<&RawValue>(body) else {
        return Ok(None);
    };
    if !claude_json::valid(raw, work)? {
        return Ok(None);
    }
    let items = if raw.get().starts_with('{') {
        vec![raw]
    } else if let Some(items) = claude_json::array(raw) {
        items
    } else {
        return Ok(None);
    };
    if items.is_empty() {
        return Ok(None);
    }
    let mut call_id = String::new();
    let mut answers = Vec::new();
    for raw in items {
        work.check()?;
        let Some(item) = Object::parse(raw) else {
            return Ok(None);
        };
        let encoded_id = item.text("questionItemId");
        let Ok(id) = serde_json::from_str::<&RawValue>(&encoded_id) else {
            return Ok(None);
        };
        if !claude_json::valid(id, work)? {
            return Ok(None);
        }
        let Some(id) = claude_json::array(id) else {
            return Ok(None);
        };
        let Some(id) = id
            .get(1)
            .and_then(|raw| serde_json::from_str::<String>(raw.get()).ok())
        else {
            return Ok(None);
        };
        if call_id.is_empty() {
            call_id = id;
        }
        answers.push(Answer {
            question: item.text("question"),
            answer: item.text("answer"),
        });
    }
    Ok(Some((call_id, answers)))
}

pub(super) fn output(text: &str, work: &mut dyn Work) -> Result<QuestionOutput, SnapshotError> {
    if text == "{\"accepted\":true}" {
        return Ok(QuestionOutput::Accepted);
    }
    if !text.starts_with("{\"answers\":") {
        return Ok(QuestionOutput::None);
    }
    let Ok(raw) = serde_json::from_str::<&RawValue>(text) else {
        return Ok(QuestionOutput::None);
    };
    if !claude_json::valid(raw, work)? {
        return Ok(QuestionOutput::None);
    }
    let Some(object) = Object::parse(raw) else {
        return Ok(QuestionOutput::None);
    };
    if object.fields().count() != 1 {
        return Ok(QuestionOutput::None);
    }
    let Some(answers) = object.object("answers") else {
        return Ok(QuestionOutput::None);
    };
    let mut result = Vec::new();
    for (id, raw) in answers.fields() {
        work.check()?;
        let Some(picked) = Object::parse(raw).and_then(|obj| obj.array("answers")) else {
            return Ok(QuestionOutput::None);
        };
        let mut parts = Vec::new();
        for raw in picked {
            work.check()?;
            let Ok(text) = serde_json::from_str::<String>(raw.get()) else {
                return Ok(QuestionOutput::None);
            };
            parts.push(text);
        }
        result.push(Answer {
            question: id.to_owned(),
            answer: parts.join("\n"),
        });
    }
    Ok(QuestionOutput::Answers(result))
}
