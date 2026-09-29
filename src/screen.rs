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

/// A question on an agent's screen and the choices it offers, in screen order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Prompt {
    /// The rows above the choices, up to a rule or an earlier history row.
    pub question: String,
    pub choices: Vec<String>,
    /// Numbered choices take their number as the key; unnumbered ones take arrow keys.
    pub numbered: bool,
    /// The choice the screen's cursor marker is on.
    pub cursor: usize,
    /// A hash of the question and choices, so an answer can prove it saw this prompt.
    pub id: String,
    /// The digit only moves the cursor, so Enter confirms the choice (Codex folder trust).
    #[serde(skip)]
    pub confirm: bool,
    /// No rule or history row closes the question above, so rows of it can be off the screen.
    #[serde(skip)]
    pub cut: bool,
}

/// Rows that end a choice list on every prompt seen: Claude permission and AskUserQuestion,
/// Codex approval and folder trust, AGY folder trust and ask_question (tests/fixtures/screens).
const PROMPT_FOOTERS: &[&str] = &[
    "esc to cancel",
    "press enter to confirm",
    "enter continue",
    "enter to select",
    "enter select",
    "enter confirm",
    "tab amend",
];
const CURSORS: &[char] = &['❯', '›', '>'];
/// Rows the question does not reach past: a history entry's marker.
const HISTORY: &[char] = &['❯', '›', '>', '•', '⏺', '●', '↳', '✻', '▸'];

/// A numbered choice's number, label, and label column, as in `❯ 1. Yes` or `  2. No`.
fn numbered_choice(line: &str) -> Option<(usize, &str, usize)> {
    let rest = line.trim_start().trim_start_matches(CURSORS).trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let after = rest[digits..].strip_prefix(". ")?.trim_start();
    let label = after.trim_end();
    let column = line.chars().count() - after.chars().count();
    (!label.is_empty()).then_some((rest[..digits].parse().ok()?, label, column))
}

fn indent(line: &str) -> usize {
    line.chars().count() - line.trim_start().chars().count()
}

/// How many status rows a live prompt's footer can have below it, such as AGY's mode line.
const ROWS_BELOW_FOOTER: usize = 2;

/// The prompt a pane's screen shows: at least two choices with only blank rows between the last
/// one and a known footer row near the bottom. Numbered choices run 1, 2, … and may have a
/// description or a rule row between them; unnumbered ones (AGY folder trust) are a cursor row and
/// the rows below it at the same label column.
pub fn prompt(rows: &str) -> Option<Prompt> {
    let lines: Vec<&str> = rows.trim_end().lines().collect();
    let is_footer = |line: &str| {
        let line = line.to_lowercase();
        PROMPT_FOOTERS.iter().any(|footer| line.contains(footer))
    };
    let mut below = 0;
    // AGY's status row also says "esc to cancel", so each footer candidate near the bottom is tried.
    for index in (0..lines.len()).rev() {
        // An input line below a list makes the list history: the agent waits on text, not a
        // choice, and a digit sent to it would be typed into the input line.
        if below > ROWS_BELOW_FOOTER || lines[index].trim_start().starts_with(CURSORS) {
            return None;
        }
        if is_footer(lines[index])
            && let Some(found) = prompt_above(&lines, index)
        {
            return Some(found);
        }
        if !lines[index].trim().is_empty() {
            below += 1;
        }
    }
    None
}

/// The prompt a screen shows, read again from the pane's history when nothing closes its question
/// above. None when that read fails, or when its question is still open and the history holds
/// rows above the screen, so no prompt is shown or answered on part of its question.
pub fn whole_prompt(screen: &str, history: impl FnOnce() -> Option<String>) -> Option<Prompt> {
    let found = prompt(screen)?;
    if !found.cut {
        return Some(found);
    }
    let history = history()?;
    let whole = prompt(&history)?;
    // A history with no row above the screen (an alternate screen, or a new pane) holds the
    // whole pane, so its open question starts at its top row. `capture -J` keeps trailing
    // blanks, and a wrapped row it joins makes the two differ, so such a pane is refused.
    let rows = |text: &str| {
        let rows: Vec<&str> = text.lines().map(str::trim_end).collect();
        rows.join("\n").trim_matches('\n').to_string()
    };
    (!whole.cut || rows(&history) == rows(screen)).then_some(whole)
}

fn prompt_above(lines: &[&str], footer: usize) -> Option<Prompt> {
    let last = (0..footer)
        .rev()
        .find(|&index| !lines[index].trim().is_empty())?;
    // The last choice can wrap onto rows below it.
    let last_choice = (last.saturating_sub(3)..=last)
        .rev()
        .find(|&row| numbered_choice(lines[row]).is_some());
    let (first, choices, numbered, cursor) = if let Some(mut index) = last_choice {
        let mut number = numbered_choice(lines[index])?.0;
        let mut rows = vec![index];
        while number > 1 {
            let found = (index.saturating_sub(6)..index)
                .rev()
                .find(|&row| numbered_choice(lines[row]).is_some_and(|(n, ..)| n == number - 1))?;
            rows.push(found);
            index = found;
            number -= 1;
        }
        rows.reverse();
        // A label takes every row up to the next choice, so a wrapped path or a description
        // is shown and hashed with it. A row left of the label column is not the label's, so
        // the list is not a prompt.
        let ends = rows[1..].iter().copied().chain([footer]);
        let labels: Vec<String> = rows
            .iter()
            .zip(ends)
            .map(|(&row, end)| {
                let (_, label, column) = numbered_choice(lines[row])?;
                let mut parts = vec![label];
                for line in &lines[row + 1..end] {
                    let text = line.trim();
                    if text.is_empty() || text.starts_with('─') {
                        continue;
                    }
                    if indent(line) < column {
                        return None;
                    }
                    parts.push(text);
                }
                Some(parts.join("\n"))
            })
            .collect::<Option<_>>()?;
        let cursor = rows
            .iter()
            .position(|&row| lines[row].trim_start().starts_with(CURSORS))
            .unwrap_or(0);
        (rows[0], labels, true, cursor)
    } else {
        // The column a row's label starts in, past a cursor marker and its space.
        let label_column = |line: &str| {
            let text = line.trim_start();
            let indent = line.chars().count() - text.chars().count();
            match text.strip_prefix(CURSORS) {
                Some(rest) => {
                    indent + 1 + (text.chars().count() - 1 - rest.trim_start().chars().count())
                }
                None => indent,
            }
        };
        let column = label_column(lines[last]);
        let mut first = last;
        while first > 0
            && !lines[first - 1].trim().is_empty()
            && label_column(lines[first - 1]) == column
        {
            first -= 1;
        }
        let rows = first..=last;
        let cursors: Vec<usize> = rows
            .clone()
            .filter(|&row| lines[row].trim_start().starts_with(CURSORS))
            .map(|row| row - first)
            .collect();
        let [cursor] = cursors[..] else {
            return None;
        };
        let labels: Vec<String> = rows
            .map(|row| {
                lines[row]
                    .trim()
                    .trim_start_matches(CURSORS)
                    .trim()
                    .to_string()
            })
            .collect();
        (first, labels, false, cursor)
    };
    if choices.len() < 2 || choices.iter().any(String::is_empty) {
        return None;
    }
    // The whole question up to a rule or a history row, so no row of a long command is left
    // out of the card or the id. Both start at column 0 on every capture; the question's own
    // rows are indented, so a command row such as `> /tmp/out` stays in the question.
    let mut question = Vec::new();
    let mut bounded = false;
    for line in lines[..first].iter().rev() {
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        if line.starts_with('─') || line.starts_with(HISTORY) {
            bounded = true;
            break;
        }
        question.push(text);
    }
    question.reverse();
    let question = question.join("\n");
    // FNV-1a: stable across builds, so a listing and a later answer agree.
    let id = std::iter::once(question.as_str())
        .chain(choices.iter().map(String::as_str))
        .flat_map(|part| part.bytes().chain([0]))
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    Some(Prompt {
        question,
        choices,
        numbered,
        cursor,
        id: format!("{id:016x}"),
        // Codex's folder trust footer; there a digit moves the cursor and Enter picks.
        confirm: numbered && lines[footer].to_lowercase().contains("enter continue"),
        cut: !bounded,
    })
}

impl Prompt {
    /// The tmux keys that pick choice `index`: its number, or arrows from the cursor then Enter.
    pub fn keys(&self, index: usize) -> Option<Vec<String>> {
        if index >= self.choices.len() {
            return None;
        }
        if self.numbered {
            return Some(vec![(index + 1).to_string()]);
        }
        let arrow = if index > self.cursor { "Down" } else { "Up" };
        let mut keys = vec![arrow.to_string(); index.abs_diff(self.cursor)];
        keys.push("Enter".to_string());
        Some(keys)
    }
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
    fn every_prompt_shape_gives_its_question_choices_cursor_and_keys() {
        let cases = [
            (
                fixture!("claude-waiting"),
                "Bash command\n│ touch /tmp/work/probe.txt\nCreate empty probe file\nDo you want to proceed?",
                &[
                    "Yes",
                    "Yes, and always allow access to\n/tmp/work from this project",
                    "No",
                ][..],
                true,
            ),
            (
                fixture!("claude-question"),
                "☐ Color\nWhich color do you prefer?",
                &[
                    "Red\nThe color red",
                    "Blue\nThe color blue",
                    "Type something.",
                    "Chat about this",
                ],
                true,
            ),
            (
                fixture!("codex-waiting"),
                "Would you like to run the following command?\nEnvironment: local\n\
                 Reason: Do you approve running touch probe.txt outside the sandbox?\n$ touch probe.txt",
                &[
                    "Yes, proceed (y)",
                    "Yes, and don't ask again for commands that start with `touch probe.txt` (p)",
                    "No, and tell Codex what to do differently (esc)",
                ],
                true,
            ),
            (
                fixture!("codex-trust"),
                "Folder access\n/tmp/work\nTrust this folder? Codex can read, edit, and run files here, \
                 subject to your permission settings. Folder\nsettings can run code automatically, even \
                 without a model request. Continue only if you trust these files.\nYour trust decision \
                 will be saved.",
                &["Trust and continue", "Back to Agent Command Center"],
                true,
            ),
            (
                fixture!("agy-question"),
                "Question 1/1: Which color do you choose?",
                &["Red", "Blue", "Write-in..."],
                true,
            ),
            (
                fixture!("agy-trust"),
                "Accessing workspace:\n/tmp/work\nDo you trust the contents of this project?\n\
                 Antigravity CLI requires permission to read, edit, and execute files here.",
                &["Yes, I trust this folder", "No, exit"],
                false,
            ),
            (
                fixture!("agy-waiting"),
                "Agent is requesting permission for: run_command\ntouch /tmp/work/probe.txt\n\
                 Do you want to proceed?",
                &["Yes", "No"],
                false,
            ),
        ];
        for (rows, question, choices, numbered) in cases {
            let prompt = prompt(rows).unwrap_or_else(|| panic!("no prompt in:\n{rows}"));
            assert_eq!(prompt.question, question, "{rows}");
            assert_eq!(prompt.choices, choices, "{rows}");
            assert_eq!((prompt.numbered, prompt.cursor), (numbered, 0), "{rows}");
        }

        let claude = prompt(fixture!("claude-waiting")).unwrap();
        assert_eq!(claude.keys(2), Some(vec!["3".to_string()]));
        assert_eq!(claude.keys(3), None);
        let agy = prompt(fixture!("agy-trust")).unwrap();
        assert_eq!(agy.keys(0), Some(vec!["Enter".to_string()]));
        assert_eq!(
            agy.keys(1),
            Some(vec!["Down".to_string(), "Enter".to_string()])
        );
        // A moved cursor changes the arrows, and the id, so a stale answer is refused.
        let moved = prompt(
            &fixture!("agy-trust")
                .replace("> Yes, I trust", "  Yes, I trust")
                .replace("  No, exit", "> No, exit"),
        )
        .unwrap();
        assert_eq!(moved.cursor, 1);
        assert_eq!(
            moved.keys(0),
            Some(vec!["Up".to_string(), "Enter".to_string()])
        );
        assert_eq!(moved.id, agy.id);
        let other = prompt(&fixture!("claude-waiting").replace("probe.txt", "other.txt")).unwrap();
        assert_ne!(other.id, claude.id);
    }

    #[test]
    fn every_row_of_a_long_command_is_in_the_question_and_its_id() {
        let command = |first: &str| {
            let rows: String = (1..=6).map(|n| format!("   echo step {n}\n")).collect();
            format!(
                "────\n Bash command\n\n   {first}\n{rows}\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel\n"
            )
        };
        let harmless = prompt(&command("ls /tmp/work")).unwrap();
        let deleting = prompt(&command("rm -rf /tmp/work")).unwrap();
        assert!(
            deleting.question.contains("rm -rf /tmp/work"),
            "{}",
            deleting.question
        );
        assert_ne!(deleting.id, harmless.id);
        assert!(!deleting.cut);

        // A question that reaches the top row is read again from the pane's history.
        let screen = command("rm -rf /tmp/work").replace("────\n Bash command\n\n", "");
        assert!(prompt(&screen).unwrap().cut);
        let history = format!("● I will clean up.\n Bash command\n{screen}");
        let whole = whole_prompt(&screen, || Some(history.clone())).unwrap();
        assert!(
            whole.question.starts_with("Bash command\nrm -rf"),
            "{}",
            whole.question
        );
        assert_eq!(whole_prompt(&screen, || None), None);
        // History that lost the question's first rows is refused too.
        let lost = format!("   echo step 0\n{screen}");
        assert_eq!(whole_prompt(&screen, || Some(lost)), None);
        let never = || -> Option<String> { panic!("a whole question needs no history") };
        assert_eq!(whole_prompt(&command("ls"), never), prompt(&command("ls")));
    }

    #[test]
    fn a_blank_row_inside_a_command_at_the_top_row_does_not_end_the_question() {
        let command = |first: &str| {
            format!(
                "────\n Bash command\n\n   {first}\n\n   echo done\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel\n"
            )
        };
        // The pane shows the rows from the blank row below the first command row.
        let screen = |first: &str| {
            let history = command(first);
            let start = history.find("\n\n   echo done").unwrap() + 1;
            history[start..].to_string()
        };
        assert!(prompt(&screen("rm -rf /tmp/work")).unwrap().cut);
        let whole = |first: &str| whole_prompt(&screen(first), || Some(command(first))).unwrap();
        assert!(
            whole("rm -rf /tmp/work")
                .question
                .contains("rm -rf /tmp/work"),
            "{}",
            whole("rm -rf /tmp/work").question
        );
        assert_ne!(whole("rm -rf /tmp/work").id, whole("ls /tmp/work").id);
    }

    #[test]
    fn an_open_question_counts_only_when_the_history_holds_no_row_above_the_screen() {
        // A Codex folder trust pane is on the alternate screen, so its history is the screen.
        let screen = fixture!("codex-trust");
        let found = prompt(screen).unwrap();
        assert!(found.cut);
        let padded: String = screen.lines().map(|line| format!("{line}   \n")).collect();
        assert_eq!(
            whole_prompt(screen, || Some(format!("{padded}\n\n"))),
            Some(found)
        );
        assert_eq!(
            whole_prompt(screen, || Some(format!("$ codex\n{screen}"))),
            None
        );
    }

    #[test]
    fn an_indented_command_row_that_starts_like_history_stays_in_the_question() {
        let command = |first: &str| {
            format!(
                "• Ran ls\n\n  Would you like to run the following command?\n\n  $ {first}\n  > /tmp/result\n  echo done\n\n› 1. Yes, proceed (y)\n  2. No (esc)\n\n  Press enter to confirm or esc to cancel\n"
            )
        };
        let harmless = prompt(&command("printf safe")).unwrap();
        let deleting = prompt(&command("rm -rf /tmp/work")).unwrap();
        assert!(
            deleting
                .question
                .contains("$ rm -rf /tmp/work\n> /tmp/result"),
            "{}",
            deleting.question
        );
        assert_ne!(deleting.id, harmless.id);
        assert!(!deleting.cut);
    }

    #[test]
    fn a_wrapped_choice_row_is_part_of_its_label_and_id() {
        let rows = fixture!("claude-waiting");
        let other = rows.replace("      /tmp/work from", "      /etc from");
        assert_ne!(other, rows);
        assert_ne!(prompt(&other).unwrap().id, prompt(rows).unwrap().id);
        // The last choice wraps too.
        let last = rows.replace(
            "   3. No\n",
            "   3. No, and tell Claude what\n      to do instead\n",
        );
        assert_eq!(
            prompt(&last).unwrap().choices[2],
            "No, and tell Claude what\nto do instead"
        );
        // A row left of the labels is not a choice's, so the list is not a prompt.
        let stray = rows.replace("      /tmp/work from", " /tmp/work from");
        assert_eq!(prompt(&stray), None);
    }

    #[test]
    fn an_old_codex_list_above_the_idle_composer_is_no_prompt() {
        let rows = format!(
            "{}\n› Ask Codex to do anything\n\n  ? for shortcuts                                100% context left\n",
            fixture!("codex-waiting")
        );
        assert_eq!(prompt(&rows), None);
        assert_eq!(screen_state("codex", &rows), Some(ScreenState::Idle));
    }

    #[test]
    fn only_codex_folder_trust_confirms_its_digit_with_enter() {
        assert!(prompt(fixture!("codex-trust")).unwrap().confirm);
        for rows in [
            fixture!("claude-waiting"),
            fixture!("codex-waiting"),
            fixture!("agy-question"),
            fixture!("agy-trust"),
        ] {
            assert!(!prompt(rows).unwrap().confirm, "{rows}");
        }
    }

    #[test]
    fn a_screen_with_no_footer_below_its_list_has_no_prompt() {
        for rows in [
            fixture!("claude-idle"),
            fixture!("claude-after-esc"),
            fixture!("claude-working"),
            fixture!("claude-done"),
            fixture!("codex-idle"),
            fixture!("codex-working"),
            fixture!("codex-failed"),
            fixture!("agy-idle"),
            fixture!("agy-working"),
            // A numbered answer with the working footer rows below it.
            "• Plan\n  1. Read\n  2. Edit\n────\n>\n────\nesc to cancel      Gemini 3.7 Flash\n",
            // A cursor history row with a wrapped second row, as Claude shows a sent message.
            "❯ Use the tool to ask me\n  one question\n────\n",
        ] {
            assert_eq!(prompt(rows), None, "{rows}");
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
        // Build the draft from the fixture's own input line, which has U+00A0 after the ❯.
        let fixture = fixture!("claude-idle");
        let input = fixture.lines().find(|line| line.starts_with('❯')).unwrap();
        let claude = fixture.replace(input, "❯\u{a0}1. rerun the tests");
        assert_ne!(claude, fixture);
        assert_eq!(screen_state("claude", &claude), Some(ScreenState::Idle));
        let asked = format!("● Do you want to add a test?\n\n{claude}");
        assert_eq!(screen_state("claude", &asked), Some(ScreenState::Idle));
        let codex =
            fixture!("codex-idle").replace("› Ask Codex to do anything", "› 1. rerun the tests");
        assert_ne!(codex, fixture!("codex-idle"));
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
