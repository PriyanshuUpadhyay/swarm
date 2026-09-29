//! The screen check of ADR 0021: read an agent pane's bottom rows where a provider sends no hook,
//! such as an AGY approval prompt or the idle prompt after Esc, and decide which report to show.

/// How long a hook report is the authority over the screen, in seconds.
pub const HOOK_AUTHORITY_S: i64 = 10;
/// A `working` or `waiting` report older than this, in seconds, that nothing confirms is shown
/// as no report.
pub const STALE_S: i64 = 45;
/// How many bottom rows of a pane the classifier reads.
pub const ROWS: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    /// A spinner or an interrupt hint shows a turn in progress.
    Working,
    Waiting,
    Idle,
}

impl ScreenState {
    pub fn as_state(self) -> &'static str {
        match self {
            ScreenState::Working => "working",
            ScreenState::Waiting => "waiting",
            ScreenState::Idle => "done",
        }
    }
}

const CLAUDE_SPINNERS: &[char] = &['*', '·', '✢', '✳', '✶', '✻', '✽'];

/// True for a numbered choice row such as `❯ 1. Yes` or `  2. No`, after an optional marker.
fn is_option(line: &str, marker: char) -> bool {
    let rest = line.trim_start().trim_start_matches(marker).trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with('.')
}

/// An approval prompt: a question row, then a choice row or a footer row below it, and no idle input
/// prompt after the question. A question that is only text in a finished answer has the idle
/// prompt below it, so it does not count.
fn approval(
    lines: &[&str],
    questions: &[&str],
    footers: &[&str],
    marker: char,
    idle: impl Fn(usize) -> bool,
) -> bool {
    let Some(question) = lines.iter().rposition(|line| {
        let line = line.to_lowercase();
        questions.iter().any(|question| line.contains(question))
    }) else {
        return false;
    };
    let below = &lines[question..];
    let asks = below.iter().any(|line| {
        let line_lower = line.to_lowercase();
        is_option(line, marker) || footers.iter().any(|footer| line_lower.contains(footer))
    });
    asks && !(question + 1..lines.len()).any(idle)
}

/// The state the bottom rows of a pane show, or None when they show no prompt that needs the
/// user, no turn in progress, and no idle input prompt.
pub fn screen_state(provider: &str, rows: &str) -> Option<ScreenState> {
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
                    && !is_option(lines[index], '❯')
            };
            let questions = ["do you want to", "waiting for permission"];
            if approval(lines, &questions, &["esc to cancel"], '❯', idle_at) {
                return Some(ScreenState::Waiting);
            }
            let spinner = lines.iter().any(|line| {
                first(line).is_some_and(|glyph| CLAUDE_SPINNERS.contains(&glyph))
                    && line.contains('…')
            });
            if spinner || has(&["esc to interrupt"]) {
                return Some(ScreenState::Working);
            }
            (0..lines.len()).any(idle_at).then_some(ScreenState::Idle)
        }
        "codex" => {
            let idle_at = |index: usize| {
                lines[index].trim_start().starts_with('›') && !is_option(lines[index], '›')
            };
            let questions = ["would you like to", "to submit answer"];
            // A request for input has its footer on the question row itself.
            let footers = ["press enter to confirm", "to submit answer"];
            if approval(lines, &questions, &footers, '›', idle_at) {
                return Some(ScreenState::Waiting);
            }
            if has(&["esc to interr", "• working ("]) {
                return Some(ScreenState::Working);
            }
            (0..lines.len()).any(idle_at).then_some(ScreenState::Idle)
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
        _ => None,
    }
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

/// What the listing shows for one agent, and the state to write, if any.
///
/// A hook report younger than `HOOK_AUTHORITY_S` wins. After that, a screen result that differs
/// is written with source `screen`. An idle screen agrees with `failed`, because a failed turn
/// also ends at the idle prompt. A `working` or `waiting` report older than `STALE_S` that the
/// screen does not confirm is shown as no report, and nothing is written for it.
pub fn resolve(
    state: Option<&str>,
    state_at: Option<i64>,
    source: Option<&str>,
    screen: Option<ScreenState>,
    now: i64,
) -> (Option<String>, Option<&'static str>) {
    let age = state_at.map(|at| now - at);
    if source == Some("hook") && age.is_some_and(|age| age < HOOK_AUTHORITY_S) {
        return (state.map(str::to_string), None);
    }
    if let Some(screen) = screen {
        let seen = screen.as_state();
        let agrees = state == Some(seen) || (seen == "done" && state == Some("failed"));
        if !agrees {
            return (Some(seen.to_string()), Some(seen));
        }
        return (state.map(str::to_string), None);
    }
    let stale = matches!(state, Some("working" | "waiting")) && age.is_none_or(|age| age > STALE_S);
    (state.filter(|_| !stale).map(str::to_string), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../tests/fixtures/screens/", $name, ".txt"))
        };
    }

    #[test]
    fn classifies_every_fixture() {
        use ScreenState::{Idle, Waiting, Working};
        let cases = [
            ("claude", fixture!("claude-idle"), Some(Idle)),
            ("claude", fixture!("claude-done"), Some(Idle)),
            ("claude", fixture!("claude-after-esc"), Some(Idle)),
            ("claude", fixture!("claude-waiting"), Some(Waiting)),
            ("claude", fixture!("claude-working"), Some(Working)),
            ("codex", fixture!("codex-idle"), Some(Idle)),
            ("codex", fixture!("codex-waiting"), Some(Waiting)),
            ("codex", fixture!("codex-working"), Some(Working)),
            ("agy", fixture!("agy-idle"), Some(Idle)),
            ("agy", fixture!("agy-waiting"), Some(Waiting)),
            ("agy", fixture!("agy-working"), Some(Working)),
            ("claude", "", None),
            ("claude", "$ ls\nsrc\n", None),
            ("gemini", fixture!("claude-idle"), None),
        ];
        for (provider, rows, expected) in cases {
            assert_eq!(
                screen_state(provider, rows),
                expected,
                "{provider}:\n{rows}"
            );
        }
    }

    #[test]
    fn a_question_in_an_answer_above_the_idle_prompt_is_not_an_approval() {
        let answer = "● Done. Do you want to add a test for it?\n\n";
        let rows = format!("{answer}{}", fixture!("claude-idle"));
        assert_eq!(screen_state("claude", &rows), Some(ScreenState::Idle));
        let codex = format!(
            "• Would you like to run the suite next?\n{}",
            fixture!("codex-idle")
        );
        assert_eq!(screen_state("codex", &codex), Some(ScreenState::Idle));
        // Codex's request for input keeps its footer on one row, with no choice rows.
        let asking = "  Which branch should I use?\n\n  enter to submit answer · esc to cancel\n";
        assert_eq!(screen_state("codex", asking), Some(ScreenState::Waiting));
    }

    #[test]
    fn reads_only_the_bottom_rows() {
        let old_prompt = format!("{}{}", fixture!("claude-waiting"), "text\n".repeat(ROWS));
        assert_eq!(screen_state("claude", &old_prompt), None);
    }

    #[test]
    fn maps_herdr_agent_status() {
        let status = |value: &str| {
            herdr_state(&format!(
                r#"{{"result":{{"agent":{{"agent_status":"{value}"}}}}}}"#
            ))
        };
        assert_eq!(status("idle"), Some(ScreenState::Idle));
        assert_eq!(status("working"), Some(ScreenState::Working));
        assert_eq!(status("blocked"), Some(ScreenState::Waiting));
        assert_eq!(status("unknown"), None);
        assert_eq!(herdr_state("not json"), None);
    }

    #[test]
    fn a_fresh_hook_wins_then_a_differing_screen_is_written() {
        let now = 1_000;
        let waiting = Some(ScreenState::Waiting);
        let idle = Some(ScreenState::Idle);
        // A hook report 9 s old is the authority, whatever the screen shows.
        assert_eq!(
            resolve(Some("working"), Some(now - 9), Some("hook"), waiting, now),
            (Some("working".into()), None)
        );
        // At 10 s the screen wins: Esc after a permission prompt sends no hook.
        assert_eq!(
            resolve(Some("waiting"), Some(now - 10), Some("hook"), idle, now),
            (Some("done".into()), Some("done"))
        );
        // A screen that agrees writes nothing.
        assert_eq!(
            resolve(Some("waiting"), Some(now - 30), Some("hook"), waiting, now),
            (Some("waiting".into()), None)
        );
        assert_eq!(
            resolve(Some("failed"), Some(now - 30), Some("hook"), idle, now),
            (Some("failed".into()), None)
        );
        // An agent with no report yet that sits at its prompt is done.
        assert_eq!(
            resolve(None, None, None, idle, now),
            (Some("done".into()), Some("done"))
        );
        // A screen source has no authority window.
        assert_eq!(
            resolve(Some("waiting"), Some(now - 1), Some("screen"), idle, now),
            (Some("done".into()), Some("done"))
        );
    }

    #[test]
    fn a_working_screen_keeps_a_long_turn_and_corrects_a_stale_report() {
        let now = 1_000;
        let working = Some(ScreenState::Working);
        assert_eq!(
            resolve(Some("working"), Some(now - 60), Some("hook"), working, now),
            (Some("working".into()), None)
        );
        for stale in ["waiting", "done"] {
            assert_eq!(
                resolve(Some(stale), Some(now - 11), Some("hook"), working, now),
                (Some("working".into()), Some("working")),
                "{stale}"
            );
        }
        assert_eq!(
            resolve(Some("done"), Some(now - 5), Some("hook"), working, now),
            (Some("done".into()), None)
        );
    }

    #[test]
    fn an_old_unconfirmed_turn_is_shown_as_no_report() {
        let now = 1_000;
        assert_eq!(
            resolve(Some("working"), Some(now - 45), Some("hook"), None, now),
            (Some("working".into()), None)
        );
        assert_eq!(
            resolve(Some("working"), Some(now - 46), Some("hook"), None, now),
            (None, None)
        );
        assert_eq!(
            resolve(Some("waiting"), Some(now - 46), Some("screen"), None, now),
            (None, None)
        );
        assert_eq!(
            resolve(
                Some("waiting"),
                Some(now - 300),
                Some("hook"),
                Some(ScreenState::Waiting),
                now
            ),
            (Some("waiting".into()), None)
        );
        assert_eq!(
            resolve(Some("done"), Some(now - 300), Some("hook"), None, now),
            (Some("done".into()), None)
        );
        assert_eq!(
            resolve(
                Some("working"),
                Some(now - 300),
                Some("hook"),
                Some(ScreenState::Working),
                now
            ),
            (Some("working".into()), None)
        );
    }
}
