//! Read the text a running turn shows on screen (ADR 0131).

/// The last block of the running turn that `provider`'s CLI draws above its input box, without
/// its block mark, or None when the block is a tool call, when nothing is drawn yet, when the
/// screen still shows an ended turn, or when swarm cannot read that CLI. A preview only: the log
/// stays the record of the turn. AGY draws its answer only at the end of a turn, so its one
/// spinner row is its preview. `prompt` is the turn's message from the log; pi marks neither its
/// echo nor its text, so its text starts under that message.
pub fn turn_text(provider: &str, rows: &str, prompt: Option<&str>) -> Option<String> {
    let lines: Vec<&str> = rows.lines().collect();
    // The input box and the echo of a sent message start with the same mark.
    let (bottom, echo, mark) = match provider {
        "claude" => (rposition(&lines, |line| line.starts_with('❯'))?, '❯', "⏺"),
        "codex" => (rposition(&lines, |line| line.starts_with('›'))?, '›', "•"),
        // ponytail: pi marks no block, so its text starts after its last tool's `Took` row; a
        // turn with no tool shows nothing, because its own message sits above with no mark.
        "pi" => {
            let bottom = rposition(&lines, |line| {
                line.starts_with("── ") && line.contains("Working")
            })?;
            let echo = prompt.and_then(|prompt| echo_end(&lines[..bottom], prompt));
            let took = rposition(&lines[..bottom], |line| line.starts_with(" Took "));
            let top = echo.max(took)?;
            let text: Vec<&str> = lines[top + 1..bottom]
                .iter()
                .map(|line| line.trim())
                .collect();
            return joined(&text);
        }
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

/// The row where pi's echo of `prompt` ends: the lowest row that, with the rows above it, spells
/// the whole message. pi wraps at spaces, so the rows joined by one space give it back.
fn echo_end(lines: &[&str], prompt: &str) -> Option<usize> {
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let prompt = words(prompt);
    if prompt.is_empty() {
        return None;
    }
    (0..lines.len()).rev().find(|&end| {
        let mut tail = String::new();
        for line in lines[..=end].iter().rev() {
            let line = words(line);
            if line.is_empty() {
                continue;
            }
            tail = if tail.is_empty() {
                line
            } else {
                format!("{line} {tail}")
            };
            if tail == prompt {
                return true;
            }
            if !prompt.ends_with(&tail) {
                return false;
            }
        }
        false
    })
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
