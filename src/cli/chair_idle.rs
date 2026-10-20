//! CLI chair idle.

/// Whether the chair's pane reads idle with no question on it. A ring's Enter typed over a
/// question, such as a permission prompt, would answer it.
pub(crate) fn chair_idle(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let chair = swarm::store::orchestrator_of(connection, session_id)?;
    let Some(pane) = swarm::store::pane_of(connection, session_id, &chair)? else {
        return Ok(false);
    };
    let provider = swarm::store::provider_of(connection, session_id, &chair)?;
    let vars = [("pane", pane.as_str())];
    let capture = || adapter.capture_within(&vars, std::time::Duration::from_secs(1));
    let screen = match adapter.screen {
        Some(_) => adapter.screen(&vars, std::time::Duration::from_secs(1)),
        None => capture(),
    };
    Ok(screen.is_some_and(|screen| {
        swarm::screen::whole_prompt(&screen, capture).is_none()
            && matches!(
                swarm::screen::read_pane(provider.as_deref(), &screen, capture),
                Some((swarm::screen::ScreenState::Idle, _))
            )
    }))
}
