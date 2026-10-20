//! Read provider input boxes and detect unsent text.

use super::prompt;

/// The text in Claude's input box, the `❯` rows right under the box's top rule, joined; an empty box
/// shows only Claude's hint text. None until the box is drawn, as while the CLI starts. Claude draws the box
/// while a turn runs too, so a drawn box is not an idle pane.
pub fn claude_composer(rows: &str) -> Option<String> {
    let lines: Vec<&str> = rows.trim_end().lines().collect();
    let rule = |line: &str| line.trim_start().starts_with('─');
    let top = (1..lines.len())
        .rev()
        .find(|&index| rule(lines[index - 1]) && lines[index].trim_start().starts_with('❯'))?;
    let text: Vec<&str> = lines[top..]
        .iter()
        .take_while(|line| !rule(line))
        .map(|line| line.trim())
        .collect();
    Some(text.join(" ").trim_start_matches('❯').trim().to_string())
}

/// The text in Codex's composer: the last `›` row and the rows under it, joined. Codex draws the
/// composer under every turn, so the last `›` row is the composer, not a sent message. None until
/// it is drawn. The pattern comes from Codex's source, not a real capture (todo.md).
pub fn codex_composer(rows: &str) -> Option<String> {
    let lines: Vec<&str> = rows.trim_end().lines().collect();
    let top = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with('›'))?;
    let text: Vec<&str> = lines[top..].iter().map(|line| line.trim()).collect();
    Some(text.join(" ").trim_start_matches('›').trim().to_string())
}

/// The text in pi's input box: the rows between the last two rules, joined. Pi draws its folder
/// and model rows under the box, and while a turn runs the top rule reads `── ⠋ Working ──`.
/// None until the box is drawn.
pub fn pi_composer(rows: &str) -> Option<String> {
    let lines: Vec<&str> = rows.trim_end().lines().collect();
    let rule = |line: &&str| line.trim_start().starts_with('─');
    let bottom = lines.iter().rposition(rule)?;
    let top = lines[..bottom].iter().rposition(rule)?;
    let text: Vec<&str> = lines[top + 1..bottom]
        .iter()
        .map(|line| line.trim())
        .collect();
    Some(text.join(" ").trim().to_string())
}

/// Whether swarm can read the input box of `provider`'s CLI. AGY's idle prompt was never
/// captured, so its box is never read.
pub fn reads_composer(provider: &str) -> bool {
    matches!(provider, "claude" | "codex" | "pi")
}

/// The text in the input box of `provider`'s CLI, or None while it is not drawn or swarm cannot
/// read that CLI's box.
pub fn composer(provider: &str, rows: &str) -> Option<String> {
    match provider {
        "claude" => claude_composer(rows),
        "codex" => codex_composer(rows),
        "pi" => pi_composer(rows),
        _ => None,
    }
}

/// True when the input box of `provider`'s CLI still holds `text` unsent. The box wraps a long
/// line, so the check ignores whitespace. A question on screen is never a held ring, so an Enter
/// sent for it would answer the question.
pub fn holds(provider: &str, rows: &str, text: &str) -> bool {
    let squash = |text: &str| text.split_whitespace().collect::<String>();
    prompt(rows).is_none()
        && composer(provider, rows)
            .is_some_and(|composer| squash(&composer).contains(&squash(text)))
}
