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
    /// The turn ended on an error row, such as Codex's red `■ message`.
    Failed,
}

impl ScreenState {
    pub fn as_state(self) -> &'static str {
        match self {
            ScreenState::Working => "working",
            ScreenState::Waiting => "waiting",
            ScreenState::Idle => "done",
            ScreenState::Failed => "failed",
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
            (0..lines.len()).any(idle_at).then_some(ScreenState::Idle)
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
    (!later).then(|| {
        lines[error]
            .trim_start()
            .trim_start_matches("■ ")
            .trim()
            .to_string()
    })
}

/// What one screen read shows, with a failure's message. `screen` is the adapter's screen verb
/// output: Herdr's status JSON or the pane's bottom rows. Herdr's status has no failure, so an
/// idle Codex pane on Herdr also has its text read through `capture` for the error row; when
/// that read fails, the pane is unknown.
pub fn read_pane(
    provider: &str,
    screen: &str,
    capture: impl FnOnce() -> Option<String>,
) -> Option<(ScreenState, Option<String>)> {
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

/// What the listing shows for one agent, and the state to write, if any.
///
/// A hook report younger than `HOOK_AUTHORITY_S` wins. After that, a screen result that differs
/// is written with source `screen`. An idle screen agrees with a hook's `failed`, because a failed
/// turn also ends at the idle prompt, but not with a `failed` the screen wrote. A `working` or `waiting` report older than `STALE_S` that the
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
        // A failed turn also ends at the idle prompt, so an idle screen agrees with a failure a
        // hook reported. A failure the screen itself saw lasts only while its error row shows.
        let agrees = state == Some(seen)
            || (seen == "done" && state == Some("failed") && source != Some("screen"));
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
    fn a_codex_error_row_above_the_idle_composer_is_a_failure_with_its_message() {
        let rows = fixture!("codex-failed");
        assert_eq!(screen_state("codex", rows), Some(ScreenState::Failed));
        assert_eq!(
            failure_detail("codex", rows).as_deref(),
            Some("stream disconnected before completion: rate limit reached for gpt-5.5-codex")
        );
        // Output after the error means a later turn went on.
        let recovered = rows.replace("\n\n› Ask", "\n\n• Retried and passed.\n\n› Ask");
        assert_eq!(screen_state("codex", &recovered), Some(ScreenState::Idle));
        assert_eq!(failure_detail("codex", fixture!("codex-idle")), None);
        assert_eq!(
            failure_detail("codex", "› prompt\n■ error after the composer\n"),
            None
        );
        assert_eq!(
            resolve(
                Some("done"),
                Some(970),
                Some("hook"),
                Some(ScreenState::Failed),
                1_000
            ),
            (Some("failed".into()), Some("failed"))
        );
    }

    #[test]
    fn an_idle_herdr_status_still_reads_the_pane_for_a_codex_error_row() {
        let idle = r#"{"result":{"agent":{"agent_status":"idle"}}}"#;
        let working = r#"{"result":{"agent":{"agent_status":"working"}}}"#;
        assert_eq!(
            read_pane("codex", idle, || Some(fixture!("codex-failed").to_string())),
            Some((
                ScreenState::Failed,
                Some(
                    "stream disconnected before completion: rate limit reached for gpt-5.5-codex"
                        .into()
                )
            ))
        );
        assert_eq!(
            read_pane("codex", idle, || Some(fixture!("codex-idle").to_string())),
            Some((ScreenState::Idle, None))
        );
        // A failed or late text read proves nothing, so the pane is unknown, not idle, and a
        // failure the screen stored stays with nothing written.
        assert_eq!(read_pane("codex", idle, || None), None);
        let unknown = read_pane("codex", idle, || None).map(|(state, _)| state);
        assert_eq!(
            resolve(Some("failed"), Some(900), Some("screen"), unknown, 1_000),
            (Some("failed".into()), None)
        );
        // Only Codex has an error row to look for; other providers keep Herdr's idle.
        let no_read = || -> Option<String> { panic!("no text read for claude") };
        assert_eq!(
            read_pane("claude", idle, no_read),
            Some((ScreenState::Idle, None))
        );
        let never = || -> Option<String> { panic!("a working pane needs no text read") };
        assert_eq!(
            read_pane("codex", working, never),
            Some((ScreenState::Working, None))
        );
        // A text screen verb is classified as before.
        assert_eq!(
            read_pane("codex", fixture!("codex-failed"), || None).map(|(state, _)| state),
            Some(ScreenState::Failed)
        );
    }

    #[test]
    fn a_numbered_draft_on_the_input_line_is_the_idle_prompt() {
        let claude = fixture!("claude-idle")
            .replace("❯ Try \"fix typecheck errors\"", "❯ 1. rerun the tests");
        assert_eq!(screen_state("claude", &claude), Some(ScreenState::Idle));
        let asked = format!("● Do you want to add a test?\n\n{claude}");
        assert_eq!(screen_state("claude", &asked), Some(ScreenState::Idle));
        let codex =
            fixture!("codex-idle").replace("› Ask Codex to do anything", "› 1. rerun the tests");
        assert_eq!(screen_state("codex", &codex), Some(ScreenState::Idle));
        let asked = format!("• Would you like to run the suite next?\n{codex}");
        assert_eq!(screen_state("codex", &asked), Some(ScreenState::Idle));
        // Real approvals still wait.
        assert_eq!(
            screen_state("claude", fixture!("claude-waiting")),
            Some(ScreenState::Waiting)
        );
        assert_eq!(
            screen_state("codex", fixture!("codex-waiting")),
            Some(ScreenState::Waiting)
        );
    }

    #[test]
    fn a_codex_answer_with_a_numbered_list_and_a_numbered_draft_is_idle() {
        let rows = "• Would you like to:\n  1. Fix the flaky test\n  2. Rerun the suite\n\n› 1. Fix the flaky test\n\n  ? for shortcuts                                100% context left\n";
        assert_eq!(screen_state("codex", rows), Some(ScreenState::Idle));
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
    fn a_screen_failure_clears_on_an_idle_screen_but_a_hook_failure_stays() {
        let now = 1_000;
        let idle = Some(ScreenState::Idle);
        // The screen saw the error row; the retry finished with no hook, the row scrolled away.
        assert_eq!(
            resolve(Some("failed"), Some(now - 30), Some("screen"), idle, now),
            (Some("done".into()), Some("done"))
        );
        // A failure the provider reported by hook ends at the same idle prompt; it stays.
        assert_eq!(
            resolve(Some("failed"), Some(now - 30), Some("hook"), idle, now),
            (Some("failed".into()), None)
        );
        // The error row still showing keeps the screen's failure.
        assert_eq!(
            resolve(
                Some("failed"),
                Some(now - 30),
                Some("screen"),
                Some(ScreenState::Failed),
                now
            ),
            (Some("failed".into()), None)
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
