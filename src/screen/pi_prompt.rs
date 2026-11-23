//! Read Pi's native dialogs and Swarm's Pi plan overlay (ADR 0154).

use super::Prompt;
use super::prompt::prompt_id;

const PI_SELECT: &str = "↑↓ navigate  enter select  ";
const PI_INPUT: &str = "enter submit  ";
pub(super) const PI_PLAN_TITLE: &str = "Approve this plan?";
const PI_PLAN_END: &str = "Review the plan above before you answer.";
const PI_PLAN_FOOTER: &str = "Pi plan approval";
const PI_PLAN_CHOICES: [&str; 2] = ["Request changes", "Approve"];

fn pi_plan_overlay(screen: &str) -> bool {
    // The plan overlay owns the width and ends at the bottom of the viewport.
    screen
        .trim_end()
        .lines()
        .next_back()
        .and_then(|line| line.strip_prefix(PI_PLAN_FOOTER))
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(" |"))
}

/// The overlay's two choice rows sit just above its help row; a pane too small for them has none.
fn pi_plan_overlay_cursor(screen: &str) -> Option<usize> {
    let lines: Vec<&str> = screen.trim_end().lines().collect();
    let [.., first, second, _help] = lines.as_slice() else {
        return None;
    };
    match (first.trim_end(), second.trim_end()) {
        ("→ Request changes", "  Approve") => Some(0),
        ("  Request changes", "→ Approve") => Some(1),
        _ => None,
    }
}

/// The tools of Swarm's Pi extension (src/bus/pi_live.ts) that open a dialog for the owner.
const DIALOG_TOOLS: [&str; 2] = ["approve_plan", "ask_user_question"];

/// The name and arguments of the newest owner-dialog call in a Pi log, while it has no result.
/// The screen shows only part of a long question, so the log gives its whole text for the card.
/// Pi runs these tools one at a time, so only the newest call can be the open dialog.
fn open_call(log: Option<&std::path::Path>) -> Option<(String, serde_json::Value)> {
    let tail = super::log_prompt::log_tail(log?)?;
    let mut answered = std::collections::HashSet::new();
    for line in tail.rsplit(|&byte| byte == b'\n') {
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        let message = &record["message"];
        if message["role"] == "toolResult" {
            if let Some(id) = message["toolCallId"].as_str() {
                answered.insert(id.to_string());
            }
            continue;
        }
        if message["role"] != "assistant" {
            continue;
        }
        for block in message["content"].as_array().into_iter().flatten().rev() {
            let Some(name) = block["name"]
                .as_str()
                .filter(|name| DIALOG_TOOLS.contains(name))
            else {
                continue;
            };
            if block["type"] != "toolCall" {
                continue;
            }
            let id = block["id"].as_str().filter(|id| !id.is_empty())?;
            return (!answered.contains(id))
                .then(|| (name.to_string(), block["arguments"].clone()));
        }
    }
    None
}

/// One line of text as the extension shows a choice: no controls, and runs of white space as one
/// space. A wrapped screen label compares equal to its log text.
fn flat(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn choice_prompt(question: String, choices: Vec<String>, cursor: usize, input: bool) -> Prompt {
    Prompt {
        id: prompt_id(&question, &choices),
        question,
        choices,
        details: Vec::new(),
        numbered: false,
        cursor,
        cut: false,
        reply: None,
        input,
    }
}

/// The open plan from the log, with the choice the screen's cursor is on.
fn pi_logged_plan(log: Option<&std::path::Path>, cursor: usize) -> Option<Prompt> {
    let (name, arguments) = open_call(log)?;
    let plan = arguments["plan"].as_str()?.trim();
    if name != "approve_plan" || plan.is_empty() {
        return None;
    }
    let choices = PI_PLAN_CHOICES.map(String::from).to_vec();
    Some(choice_prompt(
        format!("{PI_PLAN_TITLE}\n\n{plan}"),
        choices,
        cursor,
        false,
    ))
}

/// The open `ask_user_question` from the log, when its top is off the screen. A choice question
/// must show the same choices on the screen, so the keys pick what the card shows.
fn pi_logged_question(
    log: Option<&std::path::Path>,
    shown: Option<(&[String], usize)>,
) -> Option<Prompt> {
    let (name, arguments) = open_call(log)?;
    let question = arguments["question"].as_str()?.trim();
    if name != "ask_user_question" || question.is_empty() {
        return None;
    }
    let logged: Option<Vec<String>> = arguments["choices"]
        .as_array()
        .map(|choices| {
            choices
                .iter()
                .map(|c| c.as_str().map(flat))
                .collect::<Option<_>>()
        })
        .unwrap_or(Some(Vec::new()))
        .filter(|choices: &Vec<String>| !choices.is_empty());
    match (shown, logged) {
        (Some((choices, cursor)), Some(logged))
            if choices.iter().map(|c| flat(c)).eq(logged.iter().cloned()) =>
        {
            Some(choice_prompt(question.to_string(), logged, cursor, false))
        }
        (None, None) => Some(choice_prompt(question.to_string(), Vec::new(), 0, true)),
        _ => None,
    }
}

/// A plan view whose choice rows or log call Swarm cannot read: no buttons, so nothing is approved
/// blind. Pi writes the call before it opens the view, so this lasts only until the next read.
fn pi_plan_unread() -> Prompt {
    let question =
        format!("{PI_PLAN_TITLE}\n\nSwarm could not read this plan from the agent's log.");
    choice_prompt(question, Vec::new(), 0, false)
}

/// The lower rule of Pi's native dialog can have its two normal footer rows below it.
fn pi_dialog_footer(lines: &[&str]) -> Option<(usize, usize)> {
    let border = lines.iter().rposition(|line| pi_border(line))?;
    if lines[border + 1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .count()
        > 2
    {
        return None;
    }
    let footer = lines[..border]
        .iter()
        .rposition(|line| !line.trim().is_empty())?;
    let hint = lines[footer].trim();
    let keys = hint
        .strip_prefix(PI_SELECT)
        .or_else(|| hint.strip_prefix(PI_INPUT))?;
    keys.ends_with(" cancel").then_some((footer, border))
}

fn pi_border(line: &str) -> bool {
    let line = line.trim_end();
    line.starts_with('─') && line.chars().all(|c| c == '─')
}

pub(super) fn pi_dialog(screen: &str) -> bool {
    pi_plan_overlay(screen) || pi_dialog_footer(&screen.lines().collect::<Vec<_>>()).is_some()
}

/// Swarm's Pi selector leaves a blank row between the question (or full plan) and the
/// choices. The screen proves the dialog is open and gives the cursor. A question or plan taller
/// than the screen comes from the log, never from scrollback.
pub(super) fn pi_prompt(screen: &str, log: Option<&std::path::Path>) -> Option<Prompt> {
    if pi_plan_overlay(screen) {
        return Some(
            pi_plan_overlay_cursor(screen)
                .and_then(|cursor| pi_logged_plan(log, cursor))
                .unwrap_or_else(pi_plan_unread),
        );
    }
    let lines: Vec<_> = screen.lines().collect();
    let (footer, bottom) = pi_dialog_footer(&lines)?;
    let top = lines[..footer]
        .iter()
        .rposition(|line| line.trim_end() == lines[bottom].trim_end());
    if lines[footer].trim().starts_with(PI_INPUT) {
        return match top {
            Some(top) => pi_input_prompt(&lines[top + 1..footer]),
            None => pi_logged_question(log, None),
        };
    }
    if !lines[footer].trim().starts_with(PI_SELECT) {
        return None;
    }
    let start = top.map_or(0, |i| i + 1);
    let last = (start..footer)
        .rev()
        .find(|&i| !lines[i].trim().is_empty())?;
    let separator = (start..last).rev().find(|&i| lines[i].trim().is_empty())?;
    let mut cursor = None;
    let mut choices: Vec<String> = Vec::new();
    for line in &lines[separator + 1..=last] {
        if let Some(label) = line.strip_prefix(" → ") {
            if cursor.replace(choices.len()).is_some() {
                return None;
            }
            choices.push(label.trim_end().to_string());
        } else if let Some(label) = line.strip_prefix("   ") {
            choices.push(label.trim_end().to_string());
        } else if let Some(continuation) = line.strip_prefix(' ') {
            // Pi's Text wraps to its own left padding, not to the label's column. Keep screen
            // line breaks so a card's id changes if the visible text changes.
            let label = choices.last_mut()?;
            label.push('\n');
            label.push_str(continuation.trim_end());
        } else {
            return None;
        }
    }
    if choices.len() < 2 || choices.iter().any(String::is_empty) {
        return None;
    }
    let cursor = cursor?;
    let question = lines[start..separator]
        .iter()
        .map(|line| line.strip_prefix(' ').unwrap_or(line).trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let question = question.trim();
    let plan = (choices == PI_PLAN_CHOICES)
        .then(|| question.strip_suffix(PI_PLAN_END))
        .flatten();
    let question = match (top, plan) {
        (Some(_), Some(body)) => body.trim_end().to_string(),
        (None, Some(_)) => {
            return Some(pi_logged_plan(log, cursor).unwrap_or_else(pi_plan_unread));
        }
        (Some(_), None) if !question.is_empty() => question.to_string(),
        (None, None) => return pi_logged_question(log, Some((&choices, cursor))),
        _ => return None,
    };
    Some(Prompt {
        id: prompt_id(&question, &choices),
        question,
        choices,
        details: Vec::new(),
        numbered: false,
        cursor,
        cut: false,
        reply: None,
        input: false,
    })
}

/// Pi's input dialog: the title (and an optional description) above one `> ` input row. A
/// dialog whose top border is off the screen comes from the log.
fn pi_input_prompt(rows: &[&str]) -> Option<Prompt> {
    let field = rows
        .iter()
        .rposition(|line| line.starts_with("> ") || *line == ">")?;
    let question = rows[..field]
        .iter()
        .map(|line| line.strip_prefix(' ').unwrap_or(line).trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let question = question.trim().to_string();
    if question.is_empty() || rows[field + 1..].iter().any(|line| !line.trim().is_empty()) {
        return None;
    }
    Some(Prompt {
        id: prompt_id(&question, &[]),
        question,
        choices: Vec::new(),
        details: Vec::new(),
        numbered: false,
        cursor: 0,
        cut: false,
        reply: None,
        input: true,
    })
}

/// The text in the input row of Pi's input dialog, or None when no such dialog is on screen.
pub fn pi_input_text(screen: &str) -> Option<String> {
    let lines: Vec<_> = screen.lines().collect();
    let (footer, _) = pi_dialog_footer(&lines)?;
    if !lines[footer].trim().starts_with(PI_INPUT) {
        return None;
    }
    let field = lines[..footer]
        .iter()
        .rposition(|line| line.starts_with("> ") || *line == ">")?;
    Some(lines[field][1..].trim().to_string())
}
