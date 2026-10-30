//! Read the text a running turn shows on screen (ADR 0131).

/// The last block of the running turn that `provider`'s CLI draws above its input box, without
/// its block mark, or None when the block is a tool call, when nothing is drawn yet, when the
/// screen still shows an ended turn, or when swarm cannot read that CLI. A preview only: the log
/// stays the record of the turn. AGY draws its answer only at the end of a turn, so its one
/// spinner row is its preview. pi marks no block, so its text comes from its extension, not here.
pub fn turn_text(provider: &str, rows: &str) -> Option<String> {
    let lines: Vec<&str> = rows.lines().collect();
    // The input box and the echo of a sent message start with the same mark.
    let (bottom, echo, mark) = match provider {
        "claude" => (rposition(&lines, |line| line.starts_with('❯'))?, '❯', "⏺"),
        "codex" => (rposition(&lines, |line| line.starts_with('›'))?, '›', "•"),
        "agy" => {
            let line = lines.iter().rev().find(|line| {
                line.chars()
                    .next()
                    .is_some_and(|c| ('\u{2800}'..='\u{28FF}').contains(&c))
            })?;
            let text = line.chars().skip(1).collect::<String>();
            return Some(text.trim().to_string()).filter(|text| !text.is_empty());
        }
        _ => return None,
    };
    // A sent message shows in the box before the CLI echoes it, so the screen still holds the
    // ended turn and the row that closed it.
    let echoed = rposition(&lines[..bottom], |line| line.starts_with(echo));
    if lines[..bottom]
        .iter()
        .enumerate()
        .any(|(index, line)| echoed.is_none_or(|echoed| index > echoed) && ended(provider, line))
    {
        return None;
    }
    // A block row is indented two columns. Status rows (`✢ Synthesizing…`, `• Working (…)`),
    // rules, and right-aligned hints below the block are skipped.
    let body = |line: &str| {
        let rest = line.strip_prefix("  ").unwrap_or("");
        // ponytail: a row indented 20 columns or more reads as a right-aligned hint, not text.
        !rest.is_empty() && line.len() - line.trim_start().len() < 20
    };
    let last = rposition(&lines[..bottom], |line| {
        body(line) || is_block_start(line, mark)
    })?;
    // A block above the latest echo, such as a message queued while the turn runs, is already in
    // the log, and the turn has drawn nothing since.
    if echoed.is_some_and(|echoed| last < echoed) {
        return None;
    }
    let mut text = Vec::new();
    for line in lines[..=last].iter().rev() {
        if is_block_start(line, mark) {
            text.push(line.trim_start_matches(mark).trim());
            break;
        }
        if !body(line) && !line.trim().is_empty() {
            // The rows under an echo are the rest of the owner's message, and no block yet.
            if line.starts_with(echo) {
                return None;
            }
            break;
        }
        text.push(line.trim());
    }
    text.reverse();
    if text
        .iter()
        .any(|line| line.starts_with('⎿') || line.starts_with('└'))
    {
        return None;
    }
    joined(&text)
}

/// The row a CLI draws when a turn ends, even a one-second or stopped one: Claude 2.1's
/// `✻ Sautéed for 1s · done 1:34 AM` (its running row has `…`), and Codex's
/// `Worked for 37s • 12:31 AM`, which Codex 0.162 draws at the end of each live turn (codex-rs
/// `chatwidget/completion.rs`).
pub(super) fn ended(provider: &str, line: &str) -> bool {
    match provider {
        "claude" => {
            line.starts_with(super::classify::CLAUDE_SPINNERS)
                && line.contains(" for ")
                && !line.contains('…')
        }
        _ => line.trim_start().starts_with("Worked for "),
    }
}

fn is_block_start(line: &str, mark: &str) -> bool {
    line.starts_with(mark) && !line.contains("esc to interrupt")
}

fn rposition(lines: &[&str], test: impl Fn(&str) -> bool) -> Option<usize> {
    lines.iter().rposition(|line| test(line))
}

/// The rows joined, blank rows kept as paragraph breaks; None when no text is left.
fn joined(lines: &[&str]) -> Option<String> {
    let text = lines.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}
