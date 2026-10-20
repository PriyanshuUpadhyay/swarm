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
    let log = swarm::store::agents(connection, session_id)?
        .into_iter()
        .find(|agent| agent.id == chair)
        .and_then(|agent| agent.log);
    let vars = [("pane", pane.as_str())];
    let capture = || adapter.capture_within(&vars, std::time::Duration::from_secs(1));
    let screen = match adapter.screen {
        Some(_) => adapter.screen(&vars, std::time::Duration::from_secs(1)),
        None => capture(),
    };
    Ok(screen.is_some_and(|screen| {
        swarm::screen::agent_prompt(
            provider.as_deref(),
            &screen,
            capture,
            log.as_deref().map(std::path::Path::new),
        )
        .is_none()
            && matches!(
                swarm::screen::read_pane(provider.as_deref(), &screen, capture),
                Some((swarm::screen::ScreenState::Idle, _))
            )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::test_support::*;

    fn tall_chair() -> (std::path::PathBuf, rusqlite::Connection, String) {
        let screen = include_str!("../../tests/fixtures/screens/claude-question-top-offscreen.txt")
            .replace("  6. Chat about this", "❯ 6. Chat about this");
        let (root, connection, session) = ring_session(
            "tall-chair",
            None,
            "ring = printf ring >> '{screen}.rings'\nscreen = cat '{screen}'",
            &screen,
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("claude", "chair-id"))).unwrap();
        let log = root.join("question.jsonl");
        std::fs::write(
            &log,
            include_str!("../../tests/fixtures/screens/claude-open-question.jsonl"),
        )
        .unwrap();
        swarm::store::set_log(&connection, &session, ORCHESTRATOR, &log).unwrap();
        (root, connection, session)
    }

    #[test]
    fn a_tall_claude_question_never_reads_as_an_idle_chair() {
        let (root, connection, session) = tall_chair();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        assert!(!chair_idle(&connection, &adapter, &session).unwrap());
        std::fs::write(
            root.join("screen"),
            include_str!("../../tests/fixtures/screens/claude-idle.txt"),
        )
        .unwrap();
        assert!(chair_idle(&connection, &adapter, &session).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_sweep_never_rings_a_report_over_a_tall_chair_question() {
        let (root, mut connection, session) = tall_chair();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection.execute_batch("UPDATE message SET delivery = 'hook', rings = 1, rung_at = unixepoch() - 30, seen_at = unixepoch() - 20").unwrap();
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "done",
            "hook",
            None,
            None,
            crate::cli::unix_now().unwrap(),
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let lines = swept(&mut connection, &root, &adapter, &session);
        assert!(lines.is_empty(), "{lines:?}");
        assert!(chair_mail(&connection, &session).is_empty());
        assert!(!root.join("screen.rings").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
