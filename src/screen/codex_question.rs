//! Read an open Codex async question from its rollout log.

use super::Prompt;
use super::prompt::prompt_id;
use serde_json::Value;
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::Path;

const OPEN: &str = "<send_user_message_question_reply>";
const CLOSE: &str = "</send_user_message_question_reply>";

/// The first unanswered question of the newest open `request_user_input_async` call in a
/// Codex log, read from the current turn's start (`turn_lines`). Codex 0.162.1 shows such a
/// question only as a collapsed `? 1 question` row, so the screen cannot name it. A reply that
/// names its `questionItemId` answers it, and the turn's end drops it, as the Codex TUI clears
/// its pending questions then.
pub(super) fn open_question(path: &Path) -> Option<Prompt> {
    // (call_id, questionItemId, question, choices) in log order.
    let mut open: Vec<(String, String, String, Vec<String>)> = Vec::new();
    for line in turn_lines(path)? {
        let Ok(record) = serde_json::from_slice::<Value>(&line) else {
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
                    open.push((call_id.to_string(), id, title.to_string(), choices));
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
                        open.retain(|(_, id, _, _)| *id != answered);
                    }
                }
            }
            _ => {}
        }
    }
    open.retain(|(_, _, question, choices)| {
        !question.trim().is_empty() && choices.iter().all(|choice| !choice.trim().is_empty())
    });
    let newest = open.last()?.0.clone();
    let (_, reply, question, choices) = open.into_iter().find(|(call, ..)| *call == newest)?;
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

/// Codex keeps working after it asks, and a real turn wrote 9 MB of tool output before the
/// reply, so a fixed log tail loses the question. The turn's end drops it, so the lines from the
/// last `task_started` on hold every open question. Each read scans at most `TURN_CAP` bytes.
const TURN_CAP: u64 = 64 * 1024 * 1024;
const CHUNK: u64 = 1024 * 1024;
/// Codex 0.162.1 writes a record's type, a call's name, and the start of a user message's text
/// in its first 250 bytes, so only that head is searched; a tool output can be megabytes long.
const HEAD: usize = 512;
const TURN_START: &[u8] = br#""type":"task_started""#;
const NEEDLES: &[&[u8]] = &[
    b"request_user_input_async",
    b"send_user_message_question_reply",
    b"task_complete",
    b"turn_aborted",
];

fn head_has(line: &[u8], needle: &[u8]) -> bool {
    line[..line.len().min(HEAD)]
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The byte offset of the last turn start, or the cap's floor.
fn turn_start(file: &mut std::fs::File, len: u64) -> Option<u64> {
    let floor = len.saturating_sub(TURN_CAP);
    let mut end = len;
    let mut chunk = Vec::new();
    while end > floor {
        let from = end.saturating_sub(CHUNK).max(floor);
        file.seek(SeekFrom::Start(from)).ok()?;
        chunk.clear();
        // Read past the chunk's end, so a line that starts in it shows its whole head.
        file.take(end - from + HEAD as u64)
            .read_to_end(&mut chunk)
            .ok()?;
        let span = (end - from) as usize;
        let starts = chunk[..span]
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, byte)| **byte == b'\n')
            .map(|(at, _)| at + 1)
            .chain((from == 0).then_some(0));
        for at in starts {
            if head_has(&chunk[at..], TURN_START) {
                return Some(from + at as u64);
            }
        }
        end = from;
    }
    Some(floor)
}

fn turn_lines(path: &Path) -> Option<Vec<Vec<u8>>> {
    // A FIFO must be refused before open, which would wait for a writer.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = turn_start(&mut file, len)?;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let mut lines = Vec::new();
    let mut line = Vec::new();
    // At the cap's floor the first line can be cut, and a cut record must not count.
    if start > 0 && start == len.saturating_sub(TURN_CAP) {
        reader.read_until(b'\n', &mut line).ok()?;
    }
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).ok()? == 0 {
            return Some(lines);
        }
        if NEEDLES.iter().any(|needle| head_has(&line, needle)) {
            lines.push(line.clone());
        }
    }
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
