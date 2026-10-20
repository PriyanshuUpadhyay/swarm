//! Parse pane questions and map their choices to keys.

/// A question on an agent's screen and the choices it offers, in screen order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Prompt {
    /// The rows above the choices, up to a rule or an earlier history row.
    pub question: String,
    pub choices: Vec<String>,
    /// Log-backed descriptions in choice order, with empty entries for choices without one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    /// Numbered choices take their number as the key; unnumbered ones take arrow keys.
    pub numbered: bool,
    /// The choice the screen's cursor marker is on.
    pub cursor: usize,
    /// A hash of the question and choices, so an answer can prove it saw this prompt.
    pub id: String,
    /// No rule or history row closes the question above, so rows of it can be off the screen.
    #[serde(skip)]
    pub cut: bool,
}

/// Rows that end a choice list on every prompt seen: Claude permission and AskUserQuestion,
/// Codex approval and folder trust, AGY folder trust and ask_question (tests/fixtures/screens).
pub(super) const PROMPT_FOOTERS: &[&str] = &[
    "esc to cancel",
    "press enter to confirm",
    "enter continue",
    "enter to select",
    "enter select",
    "enter confirm",
    "tab amend",
];
pub(super) const CURSORS: &[char] = &['❯', '›', '>'];
/// Rows the question does not reach past: a history entry's marker.
const HISTORY: &[char] = &['❯', '›', '>', '•', '⏺', '●', '↳', '✻', '▸'];

/// A numbered choice's number, label, and label column, as in `❯ 1. Yes` or `  2. No`.
pub(super) fn numbered_choice(line: &str) -> Option<(usize, &str, usize)> {
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
pub(super) const ROWS_BELOW_FOOTER: usize = 2;

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
/// above. None when that read fails or nothing closes the question there either: a pane can keep
/// no row above its screen, so no read proves the question starts at its top row. Such a prompt,
/// as a folder trust screen, is answered in the pane.
pub fn whole_prompt(screen: &str, history: impl FnOnce() -> Option<String>) -> Option<Prompt> {
    let found = prompt(screen)?;
    if !found.cut {
        return Some(found);
    }
    let whole = prompt(&history()?)?;
    (!whole.cut).then_some(whole)
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
    let id = prompt_id(&question, &choices);
    Some(Prompt {
        question,
        choices,
        details: Vec::new(),
        numbered,
        cursor,
        id,
        cut: !bounded,
    })
}

pub(super) fn prompt_id(question: &str, choices: &[String]) -> String {
    // FNV-1a: stable across builds, so a listing and a later answer agree.
    let id = std::iter::once(question)
        .chain(choices.iter().map(String::as_str))
        .flat_map(|part| part.bytes().chain([0]))
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("{id:016x}")
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
