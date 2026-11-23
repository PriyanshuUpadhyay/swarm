//! The screen card: an agent's question that Swarm cannot read as choices, answered with keys
//! from the chat (ADR 0156).

use super::Prompt;
use super::prompt::prompt_id;

/// The button labels of a screen card, in the order of `SCREEN_KEYS`.
const SCREEN_CHOICES: [&str; 4] = ["↑", "↓", "Enter", "Esc"];
/// The adapter key names that each button presses.
pub(super) const SCREEN_KEYS: [&str; 4] = ["Up", "Down", "Enter", "Escape"];
/// Rows of the pane, from its bottom, that the card shows. A question cut at the top of the
/// screen continues in the history rows above it.
const SCREEN_ROWS: usize = 200;

/// True when an agent waits on the owner, so a question Swarm cannot read gets a screen card.
/// Its `state` is `blocked`, or idle with a picker such as `/model` open. For a CLI whose input
/// box Swarm reads, that box must also be gone from its visible `screen`, because a dialog or a
/// picker draws in its place; so a dialog that closed leaves no card.
pub fn waits_on_owner(provider: Option<&str>, state: Option<&str>, screen: &str) -> bool {
    let reads_box = provider.filter(|provider| super::reads_composer(provider));
    match (state, reads_box) {
        // Herdr's `screen` verb prints status JSON, which shows no box.
        (Some("blocked" | "done"), Some(provider)) => {
            super::herdr_state(screen).is_none()
                && !screen.trim().is_empty()
                && !input_box_shown(provider, screen)
        }
        (Some("blocked"), None) => true,
        _ => false,
    }
}

/// A question's own rows can look like an input box, such as Codex's `› 1. Trust` row, so a
/// screen with a question on it shows no box.
fn input_box_shown(provider: &str, screen: &str) -> bool {
    match provider {
        "codex" => super::codex_input_ready(screen),
        _ => super::prompt(screen).is_none() && super::composer(provider, screen).is_some(),
    }
}

/// The screen card: the last rows of `capture`, the pane with its history, with keys and a text
/// box. A question cut at the top of the screen continues in the history rows above it.
pub fn screen_prompt(capture: &str) -> Option<Prompt> {
    let lines: Vec<&str> = capture.trim_end().lines().map(str::trim_end).collect();
    let rows = lines[lines.len().saturating_sub(SCREEN_ROWS)..].join("\n");
    let rows = rows.trim_matches('\n');
    if rows.trim().is_empty() {
        return None;
    }
    let choices: Vec<String> = SCREEN_CHOICES.map(String::from).to_vec();
    // A clock or a counter on the screen changes only digits, and it must not make every answer
    // fail as a changed question.
    let stable: String = rows.chars().filter(|c| !c.is_ascii_digit()).collect();
    Some(Prompt {
        id: prompt_id(&stable, &choices),
        question: rows.to_string(),
        choices,
        details: Vec::new(),
        numbered: false,
        cursor: 0,
        cut: false,
        reply: None,
        input: true,
        screen: true,
    })
}
