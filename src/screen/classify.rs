//! Classify provider screen rows and read pane status.

use super::{ROWS, ScreenState, pi_composer, prompt};

pub(super) const CLAUDE_SPINNERS: &[char] = &['*', '·', '✢', '✳', '✶', '✻', '✽'];

/// True for a numbered choice row such as `❯ 1. Yes` or `  2. No`, after an optional marker.
fn is_option(line: &str, marker: char) -> bool {
    let rest = line.trim_start().trim_start_matches(marker).trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with('.')
}

/// An approval prompt below its last question row: a footer row, or, where the provider's prompt
/// can lack one (`choices`), a list of at least two numbered choice rows. Returns the question row's index. One numbered
/// row alone is a draft on the input line, not a choice list.
fn approval_block(
    lines: &[&str],
    questions: &[&str],
    footers: &[&str],
    choices: Option<char>,
) -> Option<usize> {
    let question = lines.iter().rposition(|line| {
        let line = line.to_lowercase();
        questions.iter().any(|question| line.contains(question))
    })?;
    let below = &lines[question..];
    let footer = below.iter().any(|line| {
        let line = line.to_lowercase();
        footers.iter().any(|footer| line.contains(footer))
    });
    let list = choices
        .is_some_and(|marker| below.iter().filter(|line| is_option(line, marker)).count() >= 2);
    (footer || list).then_some(question)
}

/// Waiting when an approval block shows and no idle input prompt follows its question row, so a
/// question that is only text in a finished answer, with the idle prompt below it, does not
/// count.
fn approval(
    lines: &[&str],
    questions: &[&str],
    footers: &[&str],
    choices: Option<char>,
    idle: impl Fn(usize) -> bool,
) -> bool {
    approval_block(lines, questions, footers, choices)
        .is_some_and(|question| !(question + 1..lines.len()).any(idle))
}

/// The state the bottom rows of a pane show, or None when they show no prompt that needs the
/// user, no turn in progress, and no idle input prompt.
pub fn screen_state(provider: &str, rows: &str) -> Option<ScreenState> {
    // A folder trust or a question waits on the user as an approval does.
    if prompt(rows).is_some() {
        return Some(ScreenState::Waiting);
    }
    let lines: Vec<&str> = rows.trim_end().lines().collect();
    let lines = &lines[lines.len().saturating_sub(ROWS)..];
    let lower = lines.join("\n").to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));
    let first = |line: &&str| line.trim_start().chars().next();
    match provider {
        "claude" => {
            // The input line sits right under the prompt box's top rule.
            let idle_at = |index: usize| {
                index > 0
                    && lines[index - 1].trim_start().starts_with('─')
                    && lines[index].trim_start().starts_with('❯')
            };
            let questions = ["do you want to", "waiting for permission"];
            if approval(lines, &questions, &["esc to cancel"], Some('❯'), idle_at) {
                return Some(ScreenState::Waiting);
            }
            let spinner = lines.iter().any(|line| {
                first(line).is_some_and(|glyph| CLAUDE_SPINNERS.contains(&glyph))
                    && line.contains('…')
            });
            if spinner || has(&["esc to interrupt"]) {
                return Some(ScreenState::Working);
            }
            (0..lines.len()).any(idle_at).then(|| {
                if writing(rows, '❯', "claude") {
                    ScreenState::Working
                } else {
                    ScreenState::Idle
                }
            })
        }
        "codex" => {
            let questions = ["would you like to", "to submit answer"];
            // A request for input has its footer on the question row itself.
            let footers = ["press enter to confirm", "to submit answer"];
            // Every Codex approval ends with its footer (codex-rs `approval_modal_exec` snapshot:
            // "Press enter to confirm or esc to cancel"), so a numbered list alone is only text.
            let block = approval_block(lines, &questions, &footers, None);
            // A numbered row is a choice only inside a real approval block; otherwise it is a
            // draft on the composer line.
            let idle_at = |index: usize| {
                lines[index].trim_start().starts_with('›')
                    && !(block.is_some_and(|question| index > question)
                        && is_option(lines[index], '›'))
            };
            if approval(lines, &questions, &footers, None, idle_at) {
                return Some(ScreenState::Waiting);
            }
            if has(&["esc to interr", "• working ("]) {
                return Some(ScreenState::Working);
            }
            if !(0..lines.len()).any(idle_at) {
                return None;
            }
            Some(if codex_failure(lines).is_some() {
                ScreenState::Failed
            } else if writing(rows, '›', "codex") {
                ScreenState::Working
            } else {
                ScreenState::Idle
            })
        }
        "agy" => {
            if has(&["requesting permission for:"])
                && has(&["do you want to proceed?", "tab amend", "edit command"])
            {
                return Some(ScreenState::Waiting);
            }
            let spinner = lines.iter().any(|line| {
                let mut chars = line.trim_start().chars();
                chars
                    .next()
                    .is_some_and(|c| ('\u{2800}'..='\u{28FF}').contains(&c))
                    && chars.next() == Some(' ')
            });
            if spinner {
                return Some(ScreenState::Working);
            }
            // ponytail: AGY's idle prompt was never captured, so any screen without a spinner or
            // an approval counts as idle, as Herdr decides; add a prompt pattern from a real capture.
            (!lower.trim().is_empty()).then_some(ScreenState::Idle)
        }
        "pi" => {
            // While a turn runs, the top rule of pi's input box carries a spinner:
            // `── ⠋ Working ──`.
            pi_composer(rows)?;
            let spinner = lines.iter().any(|line| {
                line.trim_start().starts_with('─')
                    && line.chars().any(|c| ('\u{2800}'..='\u{28FF}').contains(&c))
            });
            Some(if spinner {
                ScreenState::Working
            } else {
                ScreenState::Idle
            })
        }
        _ => None,
    }
}

/// The message of a failed turn that the bottom rows show, for the state's detail.
pub fn failure_detail(provider: &str, rows: &str) -> Option<String> {
    let lines: Vec<&str> = rows.trim_end().lines().collect();
    let lines = &lines[lines.len().saturating_sub(ROWS)..];
    match provider {
        "codex" => codex_failure(lines),
        _ => None,
    }
}

/// Claude and Codex hide their working row while they write the answer, so the idle input line
/// shows under it. Each ends a turn with a row of its own (`live::ended`), and Codex a failed or
/// stopped one with a `■` row, so text under the latest echo of a sent message with neither row
/// is a turn still running. Reads the whole screen, because the echo is above the bottom rows.
/// ponytail: a CLI version with no end row reads as working once the hook report goes stale;
/// check the end rows again on a Claude or Codex upgrade.
fn writing(rows: &str, mark: char, provider: &str) -> bool {
    let lines: Vec<&str> = rows.trim_end().lines().collect();
    let Some(input) = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with(mark))
    else {
        return false;
    };
    // An approval's cursor row starts with the same mark, but it is a choice, not a message.
    let Some(echo) = lines[..input]
        .iter()
        .rposition(|line| line.starts_with(mark) && !is_option(line, mark))
    else {
        return false;
    };
    let turn = &lines[echo + 1..input];
    turn.iter().any(|line| !line.trim().is_empty())
        && !turn
            .iter()
            .any(|line| super::live::ended(provider, line) || line.trim_start().starts_with('■'))
}

/// Codex ends a failed turn with a red `■ message` row (codex-rs `new_error_event`). It counts
/// only when no agent (`•`) or user (`›`) row follows it before the idle composer.
fn codex_failure(lines: &[&str]) -> Option<String> {
    let error = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with("■ "))?;
    let composer = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with('›'))?;
    if error > composer {
        return None;
    }
    let later = lines[error + 1..composer]
        .iter()
        .any(|line| line.trim_start().starts_with(['•', '›']));
    let message = lines[error].trim_start().trim_start_matches("■ ").trim();
    // The owner's own stop draws the same red row, and it is no failure.
    (!later && !message.starts_with("Conversation interrupted")).then(|| message.to_string())
}

/// What one screen read shows, with a failure's message. `screen` is the adapter's screen verb
/// output: Herdr's status JSON or the pane's bottom rows. Herdr's status has no failure, so an
/// idle Codex pane on Herdr also has its text read through `capture` for the error row; when
/// that read fails, the pane is unknown. A pane with no known provider has no text patterns, so
/// only Herdr's status reads.
pub fn read_pane(
    provider: Option<&str>,
    screen: &str,
    capture: impl FnOnce() -> Option<String>,
) -> Option<(ScreenState, Option<String>)> {
    let Some(provider) = provider else {
        return herdr_state(screen).map(|state| (state, None));
    };
    if let Some(state) = herdr_state(screen) {
        // Only Codex shows a failure as an error row; an idle status needs no text otherwise.
        if state != ScreenState::Idle || provider != "codex" {
            return Some((state, None));
        }
        // A read that fails or runs late proves nothing, so the pane is unknown, not idle.
        let text = capture()?;
        return Some(match failure_detail(provider, &text) {
            Some(detail) => (ScreenState::Failed, Some(detail)),
            None => (state, None),
        });
    }
    let state = screen_state(provider, screen)?;
    Some((state, failure_detail(provider, screen)))
}

/// The state of a pane that `herdr agent get` reports, or None for any other output.
pub fn herdr_state(output: &str) -> Option<ScreenState> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    match value["result"]["agent"]["agent_status"].as_str()? {
        "idle" | "done" => Some(ScreenState::Idle),
        "working" => Some(ScreenState::Working),
        "blocked" => Some(ScreenState::Waiting),
        _ => None,
    }
}
