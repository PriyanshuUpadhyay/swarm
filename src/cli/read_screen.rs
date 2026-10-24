//! CLI read screen.

/// A pane's state, its detail, and the question on it, read from the screen (ADR 0021).
pub(crate) type ScreenRead = (
    swarm::screen::ScreenState,
    Option<String>,
    Option<swarm::screen::Prompt>,
);

/// How long one screen or capture read may take. A test's stand-in shell starts slowly on a
/// loaded Mac; the listing's own deadline still caps the read.
const SCREEN_READ: std::time::Duration =
    std::time::Duration::from_millis(if cfg!(test) { 10_000 } else { 300 });

/// The listing's screen check of one live pane: None when the `screen` verb or its read fails.
pub(crate) fn read_screen(
    adapter: &swarm::adapter::Adapter,
    pane: &str,
    provider: Option<&str>,
    log: Option<&str>,
) -> Option<ScreenRead> {
    let capture = || adapter.capture_within(&[("pane", pane)], SCREEN_READ);
    let output = adapter.screen(&[("pane", pane)], SCREEN_READ)?;
    let prompt =
        swarm::screen::agent_prompt(provider, &output, capture, log.map(std::path::Path::new));
    let (state, detail) = if provider == Some("claude") && prompt.is_some() {
        (swarm::screen::ScreenState::Waiting, None)
    } else {
        swarm::screen::read_pane(provider, &output, capture)?
    };
    Some((state, detail, prompt))
}
