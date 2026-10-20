//! Read a pane question with the agent's recorded log as a fallback.

use super::prompt::{CURSORS, PROMPT_FOOTERS, ROWS_BELOW_FOOTER, numbered_choice, prompt_id};
use super::{Prompt, whole_prompt};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const LOG_TAIL_BYTES: u64 = 256 * 1024;

/// The screen proves the question is live; the log fills text lost above the pane's top row.
pub fn agent_prompt(
    provider: Option<&str>,
    screen: &str,
    history: impl FnOnce() -> Option<String>,
    log: Option<&Path>,
) -> Option<Prompt> {
    if let Some(found) = whole_prompt(screen, history) {
        return Some(found);
    }
    if provider != Some("claude") {
        return None;
    }
    let (tail, cursor) = visible_tail(screen)?;
    let questions = open_questions(log?)?;
    let mut matches = questions.into_iter().filter_map(|question| {
        let mut choices: Vec<String> = question.options.iter().map(|o| o.label.clone()).collect();
        choices.extend(["Type something.".into(), "Chat about this".into()]);
        if question.question.trim().is_empty()
            || choices.iter().any(|choice| choice.trim().is_empty())
            || tail.last()?.0 != choices.len()
            || !tail.iter().all(|(number, label)| {
                choices
                    .get(number - 1)
                    .is_some_and(|choice| choice == label)
            })
        {
            return None;
        }
        let mut details: Vec<String> = question
            .options
            .into_iter()
            .map(|option| option.description.unwrap_or_default().trim().to_string())
            .collect();
        details.extend([String::new(), String::new()]);
        let id = prompt_id(&question.question, &choices);
        Some(Prompt {
            question: question.question,
            choices,
            details,
            numbered: true,
            cursor,
            id,
            cut: false,
        })
    });
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

fn visible_tail(screen: &str) -> Option<(Vec<(usize, &str)>, usize)> {
    let lines: Vec<&str> = screen.trim_end().lines().collect();
    let mut below = 0;
    for footer in (0..lines.len()).rev() {
        let line = lines[footer].trim_start();
        if below > ROWS_BELOW_FOOTER || line.starts_with(CURSORS) {
            return None;
        }
        if PROMPT_FOOTERS
            .iter()
            .any(|hint| line.to_lowercase().contains(hint))
        {
            let last = (0..footer)
                .rev()
                .find(|&row| !lines[row].trim().is_empty())?;
            let (mut number, _, _) = numbered_choice(lines[last])?;
            let mut rows = vec![last];
            for row in (0..last).rev() {
                if number == 1 {
                    break;
                }
                if let Some((previous, _, _)) = numbered_choice(lines[row]) {
                    if previous.checked_add(1) != Some(number) {
                        return None;
                    }
                    rows.push(row);
                    number = previous;
                }
            }
            rows.reverse();
            let mut cursor = 0;
            let mut tail = Vec::new();
            for (&row, end) in rows
                .iter()
                .zip(rows.iter().skip(1).copied().chain([footer]))
            {
                let (number, label, column) = numbered_choice(lines[row])?;
                if number == 0
                    || lines[row + 1..end].iter().any(|line| {
                        let text = line.trim();
                        !text.is_empty()
                            && !text.starts_with('─')
                            && line.chars().count() - line.trim_start().chars().count() < column
                    })
                {
                    return None;
                }
                if lines[row].trim_start().starts_with(CURSORS) {
                    cursor = number - 1;
                }
                tail.push((number, label));
            }
            return Some((tail, cursor));
        }
        if !line.is_empty() {
            below += 1;
        }
    }
    None
}

#[derive(serde::Deserialize)]
struct Question {
    question: String,
    options: Vec<Choice>,
}

#[derive(serde::Deserialize)]
struct Choice {
    label: String,
    description: Option<String>,
}

fn open_questions(path: &Path) -> Option<Vec<Question>> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let start = metadata.len().saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    file.take(LOG_TAIL_BYTES).read_to_end(&mut tail).ok()?;
    // The cap can land in the middle of a record, which must not count as a complete record.
    let tail = if start > 0 {
        &tail[tail.iter().position(|&byte| byte == b'\n')? + 1..]
    } else {
        &tail[..]
    };
    let mut answered = std::collections::HashSet::new();
    for line in tail.rsplit(|&byte| byte == b'\n') {
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        let Some(blocks) = record["message"]["content"].as_array() else {
            continue;
        };
        for block in blocks.iter().rev() {
            if record["type"] == "user" && block["type"] == "tool_result" {
                if let Some(id) = block["tool_use_id"].as_str() {
                    answered.insert(id.to_string());
                }
            } else if record["type"] == "assistant"
                && block["type"] == "tool_use"
                && block["name"] == "AskUserQuestion"
                && let Some(id) = block["id"].as_str().filter(|id| !id.is_empty())
                && !answered.contains(id)
            {
                return serde_json::from_value(block["input"]["questions"].clone()).ok();
            }
        }
    }
    None
}
