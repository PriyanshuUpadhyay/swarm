//! CLI read screen.

/// A pane's state, its detail, the question on it, and the messages its CLI holds, read from
/// the screen (ADR 0021, ADR 0130).
pub(crate) type ScreenRead = (
    swarm::screen::ScreenState,
    Option<String>,
    Option<swarm::screen::Prompt>,
    Option<Vec<swarm::screen::QueuedMessage>>,
);

/// How long one screen or capture read may take. A test's stand-in shell starts slowly on a
/// loaded Mac; the listing's own deadline still caps the read.
const SCREEN_READ: std::time::Duration =
    std::time::Duration::from_millis(if cfg!(test) { 10_000 } else { 300 });

/// The listing's screen check of one live pane: None when the `screen` verb or its read fails.
/// `state` is the agent's stored state, which decides whether a question Swarm cannot read gets
/// a screen card.
pub(crate) fn read_screen(
    adapter: &swarm::adapter::Adapter,
    pane: &str,
    provider: Option<&str>,
    log: Option<&str>,
    state: Option<&str>,
) -> Option<ScreenRead> {
    let capture = || adapter.capture_within(&[("pane", pane)], SCREEN_READ);
    let output = adapter.screen(&[("pane", pane)], SCREEN_READ)?;
    let seen = swarm::screen::read_pane(provider, &output, capture);
    let prompt = owner_prompt(provider, state, &output, capture, log);
    let read = prompt.as_ref().is_some_and(|prompt| !prompt.screen);
    let (state, detail) = match seen {
        _ if provider == Some("claude") && read => (swarm::screen::ScreenState::Blocked, None),
        Some(found) => found,
        // A screen card shows that the agent waits on the owner, also on a screen whose state
        // the reader does not know.
        None if prompt.as_ref().is_some_and(|prompt| prompt.screen) => {
            (swarm::screen::ScreenState::Blocked, None)
        }
        None => return None,
    };
    let queue = provider.and_then(|provider| swarm::screen::queue(provider, &output));
    Some((state, detail, prompt, queue))
}

/// The question an agent shows the owner: read as choices, or else a screen card when the agent
/// waits on the owner (ADR 0159). The listing, the HUD inbox, and `swarm answer` all use this, so
/// an answer proves it saw the same card.
pub(crate) fn owner_prompt(
    provider: Option<&str>,
    state: Option<&str>,
    screen: &str,
    capture: impl Fn() -> Option<String>,
    log: Option<&str>,
) -> Option<swarm::screen::Prompt> {
    let choices =
        swarm::screen::agent_prompt(provider, screen, &capture, log.map(std::path::Path::new));
    choices.or_else(|| {
        if let Some(status) = swarm::screen::herdr_state(screen) {
            let blocked = status == swarm::screen::ScreenState::Blocked
                || (status == swarm::screen::ScreenState::Idle && state == Some("blocked"));
            if !blocked {
                return None;
            }

            let rows = capture()?;
            return swarm::screen::waits_on_owner(provider, Some("blocked"), &rows)
                .then(|| swarm::screen::screen_prompt(&rows))
                .flatten();
        }

        // The screen can show a wait before any hook reports it, such as a folder trust
        // question at launch.
        let state = match swarm::screen::read_pane(provider, screen, &capture) {
            Some((swarm::screen::ScreenState::Blocked, _)) => Some("blocked"),
            _ => state,
        };
        swarm::screen::waits_on_owner(provider, state, screen)
            .then(&capture)
            .flatten()
            .and_then(|rows| swarm::screen::screen_prompt(&rows))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_regression_herdr_blocked_status_shows_a_live_card() {
        let blocked = r#"{"result":{"agent":{"agent_status":"blocked"}}}"#;
        let idle = r#"{"result":{"agent":{"agent_status":"idle"}}}"#;
        let dialog = || Some("Allow access to this folder?".to_string());
        let card = owner_prompt(Some("claude"), None, blocked, dialog, None).unwrap();
        assert!(card.screen);
        assert!(
            owner_prompt(Some("claude"), Some("blocked"), idle, dialog, None)
                .unwrap()
                .screen
        );
        assert!(owner_prompt(Some("claude"), Some("done"), idle, dialog, None).is_none());
    }
}
