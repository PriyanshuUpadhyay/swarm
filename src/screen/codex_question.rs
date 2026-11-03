//! Read an open Codex async question from its rollout log.

use super::Prompt;
use super::prompt::prompt_id;
use serde_json::Value;

const OPEN: &str = "<send_user_message_question_reply>";
const CLOSE: &str = "</send_user_message_question_reply>";

/// The first unanswered question of the newest open `request_user_input_async` call in a
/// Codex log tail. Codex 0.162.1 shows such a question only as a collapsed `? 1 question` row,
/// so the screen cannot name it. A reply that names its `questionItemId` answers it, and the
/// turn's end drops it, as the Codex TUI clears its pending questions then.
pub(super) fn open_question(tail: &[u8]) -> Option<Prompt> {
    // (questionItemId, question, choices) in log order.
    let mut open: Vec<(String, String, Vec<String>)> = Vec::new();
    for line in tail.split(|&byte| byte == b'\n') {
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let payload = &record["payload"];
        match (record["type"].as_str(), payload["type"].as_str()) {
            (Some("event_msg"), Some("task_complete" | "turn_aborted")) => open.clear(),
            (Some("response_item"), Some("function_call"))
                if payload["name"] == "request_user_input_async" =>
            {
                let Some(call_id) = payload["call_id"].as_str() else {
                    continue;
                };
                let Some(arguments) = payload["arguments"]
                    .as_str()
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                else {
                    continue;
                };
                for (index, question) in arguments["questions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let choices = question["options"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|option| option.as_str().map(String::from))
                        .collect();
                    let id = serde_json::to_string(&("request_user_input_async", call_id, index))
                        .ok()?;
                    let title = question["title"].as_str().unwrap_or_default();
                    open.push((id, title.to_string(), choices));
                }
            }
            (Some("response_item"), Some("message")) if payload["role"] == "user" => {
                for text in payload["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part["text"].as_str())
                {
                    for answered in replied_ids(text) {
                        open.retain(|(id, _, _)| *id != answered);
                    }
                }
            }
            _ => {}
        }
    }
    let (reply, question, choices) = open.into_iter().rev().find(|(_, question, choices)| {
        !question.trim().is_empty() && choices.iter().all(|choice| !choice.trim().is_empty())
    })?;
    let id = prompt_id(&question, &choices);
    Some(Prompt {
        question,
        choices,
        details: Vec::new(),
        numbered: false,
        cursor: 0,
        id,
        cut: false,
        reply: Some(reply),
    })
}

/// The `questionItemId`s a reply envelope answers, compared as parsed JSON so spacing in the
/// writer's text does not matter.
fn replied_ids(text: &str) -> Vec<String> {
    let Some(body) = text
        .trim()
        .strip_prefix(OPEN)
        .and_then(|rest| rest.strip_suffix(CLOSE))
    else {
        return Vec::new();
    };
    let items = match serde_json::from_str::<Value>(body) {
        Ok(Value::Array(items)) => items,
        Ok(item @ Value::Object(_)) => vec![item],
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| item["questionItemId"].as_str())
        .filter_map(|id| serde_json::from_str::<Value>(id).ok())
        .filter_map(|id| serde_json::to_string(&id).ok())
        .collect()
}

/// The reply envelope that answers the async question `reply` names with `answer`, as the
/// Codex TUI writes it.
pub fn codex_reply(prompt: &Prompt, answer: &str) -> Option<String> {
    let item = serde_json::json!([{
        "answer": answer,
        "question": prompt.question,
        "questionItemId": prompt.reply.as_ref()?,
    }]);
    Some(format!("{OPEN}{item}{CLOSE}"))
}
