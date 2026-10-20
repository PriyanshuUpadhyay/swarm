//! CLI read screen.

/// A pane's state, its detail, and the question on it, read from the screen (ADR 0021).
pub(crate) type ScreenRead = (
    swarm::screen::ScreenState,
    Option<String>,
    Option<swarm::screen::Prompt>,
);

/// The listing's screen check of one live pane: None when the `screen` verb or its read fails.
pub(crate) fn read_screen(
    adapter: &swarm::adapter::Adapter,
    pane: &str,
    provider: Option<&str>,
) -> Option<ScreenRead> {
    let capture =
        || adapter.capture_within(&[("pane", pane)], std::time::Duration::from_millis(300));
    let output = adapter.screen(&[("pane", pane)], std::time::Duration::from_millis(300))?;
    let prompt = swarm::screen::whole_prompt(&output, capture);
    let (state, detail) = swarm::screen::read_pane(provider, &output, capture)?;
    Some((state, detail, prompt))
}
