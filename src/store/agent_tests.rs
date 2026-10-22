//! Agent registration and pane tests.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn working_and_waiting_count_live_fresh_agents_by_provider_and_a_chair_by_its_session() {
        let connection = open(Path::new(":memory:")).unwrap();
        let session = create_session(
            &connection,
            "lane",
            Path::new("/w"),
            Some(("pi", "c1")),
            None,
        )
        .unwrap();
        for (id, provider, pane) in [
            ("orchestrator", None, Some("%1")),
            ("a", Some("claude"), Some("%2")),
            ("b", Some("claude"), Some("%3")),
            ("c", Some("codex"), Some("%4")),
            ("closed", Some("codex"), None),
        ] {
            add_agent(&connection, &session, id, "code").unwrap();
            if let Some(provider) = provider {
                set_provider(&connection, &session, id, provider).unwrap();
            }
            if let Some(pane) = pane {
                set_pane(&connection, &session, id, pane).unwrap();
            }
        }
        let now = unix_now();
        for id in ["orchestrator", "a", "b", "closed"] {
            set_state(
                &connection,
                &session,
                id,
                "working",
                "hook",
                None,
                None,
                now,
            )
            .unwrap();
        }
        // A report older than ten minutes is an agent that died mid-turn.
        set_state(
            &connection,
            &session,
            "c",
            "working",
            "hook",
            None,
            None,
            now - 601,
        )
        .unwrap();
        set_state(
            &connection,
            &session,
            "c",
            "waiting",
            "hook",
            None,
            Some("question"),
            now,
        )
        .unwrap();
        assert_eq!(
            agents_by_provider(&connection, "waiting")
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            [("codex".to_string(), 1)]
        );
        set_state(
            &connection,
            &session,
            "c",
            "working",
            "hook",
            None,
            None,
            now - 601,
        )
        .unwrap();
        let working = agents_by_provider(&connection, "working").unwrap();
        assert_eq!(
            working.into_iter().collect::<Vec<_>>(),
            [("claude".to_string(), 2), ("pi".to_string(), 1)]
        );
        set_state(
            &connection,
            &session,
            "c",
            "waiting",
            "hook",
            None,
            Some("question"),
            now - 5,
        )
        .unwrap();
        connection
            .execute(
                "UPDATE agent SET model = 'opus', account = 'team' WHERE id = 'a'",
                [],
            )
            .unwrap();
        // An older report must not move `a`: each tool call bumps `state_at`, and the HUD's tags
        // and chips would reshuffle on every poll.
        set_state(
            &connection,
            &session,
            "a",
            "working",
            "hook",
            None,
            None,
            now - 10,
        )
        .unwrap();
        let running = running_agents(&connection).unwrap();
        // Waiting first, then by session and id; the stale and closed agents are absent.
        assert_eq!(
            running
                .iter()
                .map(|agent| (
                    agent.id.as_str(),
                    agent.provider.as_str(),
                    agent.state.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                ("c", "codex", "waiting"),
                ("a", "claude", "working"),
                ("b", "claude", "working"),
                ("orchestrator", "pi", "working")
            ]
        );
        assert!(running.iter().all(|a| a.session == session));
        let claude_agent = running.iter().find(|agent| agent.id == "a").unwrap();
        assert_eq!(
            (
                claude_agent.model.as_deref(),
                claude_agent.account.as_deref()
            ),
            (Some("opus"), Some("team"))
        );
        let chair = running.iter().find(|a| a.id == "orchestrator").unwrap();
        assert_eq!(
            (chair.provider.as_str(), chair.model.as_deref()),
            ("pi", None)
        );
    }

    #[test]
    fn agent_ids_are_unique_per_session() {
        let connection = open(Path::new(":memory:")).unwrap();
        let first = create_session(&connection, "lane", Path::new("/first"), None, None).unwrap();
        let second = create_session(&connection, "lane", Path::new("/second"), None, None).unwrap();
        add_agent(&connection, &first, ORCHESTRATOR, "orchestrator").unwrap();
        add_agent(&connection, &second, ORCHESTRATOR, "orchestrator").unwrap();
        set_pane(&connection, &first, ORCHESTRATOR, "%1").unwrap();
        set_pane(&connection, &second, ORCHESTRATOR, "%2").unwrap();
        set_provider(&connection, &second, ORCHESTRATOR, "codex").unwrap();
        let second_agents = agents(&connection, &second).unwrap();
        let second_agent = &second_agents[0];
        assert_eq!(second_agent.provider.as_deref(), Some("codex"));
        assert!(second_agent.created_at > 0);
        assert!(add_agent(&connection, &second, ORCHESTRATOR, "orchestrator").is_err());
        assert!(add_agent(&connection, "missing", "ghost", "coder").is_err());
        assert_eq!(
            pane_of(&connection, &first, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%1")
        );
        assert_eq!(
            pane_of(&connection, &second, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%2")
        );
    }

    #[test]
    fn a_chair_pane_finds_its_newest_session_and_no_archived_one() {
        let mut connection = open(Path::new(":memory:")).unwrap();
        let older = create_session(
            &connection,
            "lane",
            Path::new("/older"),
            None,
            Some("herdr"),
        )
        .unwrap();
        let newer = create_session(
            &connection,
            "lane",
            Path::new("/newer"),
            None,
            Some("herdr"),
        )
        .unwrap();
        for session in [&older, &newer] {
            add_agent(&connection, session, ORCHESTRATOR, "orchestrator").unwrap();
        }
        add_agent(&connection, &older, "worker", "code.complex").unwrap();
        set_pane(&connection, &older, "worker", "wK:p2").unwrap();
        set_pane(&connection, &older, ORCHESTRATOR, "wK:p1").unwrap();
        assert_eq!(
            orchestrator_at_pane(&connection, "wK:p1").unwrap(),
            Some((older.clone(), ORCHESTRATOR.to_string()))
        );
        assert_eq!(orchestrator_at_pane(&connection, "wK:p2").unwrap(), None);

        set_pane(&connection, &newer, ORCHESTRATOR, "wK:p1").unwrap();
        assert_eq!(
            orchestrator_at_pane(&connection, "wK:p1").unwrap(),
            Some((newer.clone(), ORCHESTRATOR.to_string()))
        );
        archive_sessions(&mut connection, &[newer]).unwrap();
        assert_eq!(orchestrator_at_pane(&connection, "wK:p1").unwrap(), None);
    }

    #[test]
    fn an_orchestrator_pane_moves_only_within_one_adapter() {
        let connection = open(Path::new(":memory:")).unwrap();
        let tmux =
            create_session(&connection, "lane", Path::new("/tmux"), None, Some("tmux")).unwrap();
        let same =
            create_session(&connection, "lane", Path::new("/same"), None, Some("tmux")).unwrap();
        let solo = create_session(
            &connection,
            "lane",
            Path::new("/solo"),
            None,
            Some("tmux-swarm"),
        )
        .unwrap();
        for session in [&tmux, &same, &solo] {
            add_agent(&connection, session, ORCHESTRATOR, "code.complex").unwrap();
        }

        set_pane(&connection, &tmux, ORCHESTRATOR, "%0").unwrap();
        set_pane(&connection, &solo, ORCHESTRATOR, "%0").unwrap();
        assert_eq!(
            pane_of(&connection, &tmux, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );
        assert_eq!(
            pane_of(&connection, &solo, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );

        set_pane(&connection, &same, ORCHESTRATOR, "%0").unwrap();
        assert_eq!(pane_of(&connection, &tmux, ORCHESTRATOR).unwrap(), None);
        assert_eq!(
            pane_of(&connection, &same, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );
        assert_eq!(
            pane_of(&connection, &solo, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );
    }

    /// A stale adapter answered `self` with nothing. The empty answer was stored, and the chat
    /// said `no pane recorded` at the first message rather than at registration.
    #[test]
    fn an_empty_pane_is_refused() {
        let connection = seed();
        let refused = set_pane(&connection, SESSION, ORCHESTRATOR, "  ")
            .unwrap_err()
            .to_string();
        assert_eq!(refused, "swarm: adapter gave no pane for orchestrator");
        assert_eq!(pane_of(&connection, SESSION, ORCHESTRATOR).unwrap(), None);
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(
            pane_of(&connection, SESSION, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%1")
        );
    }

    #[test]
    fn a_command_on_an_unknown_agent_names_it() {
        let connection = seed();
        let unknown = require_pane(&connection, SESSION, "no-such-agent")
            .unwrap_err()
            .to_string();
        assert_eq!(unknown, "swarm: no agent no-such-agent in this session");
        let no_pane = require_pane(&connection, SESSION, ORCHESTRATOR)
            .unwrap_err()
            .to_string();
        assert_eq!(no_pane, "swarm: no pane recorded for orchestrator");
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(
            require_pane(&connection, SESSION, ORCHESTRATOR).unwrap(),
            "%1"
        );
    }

    #[test]
    fn sets_and_reads_pane() {
        let connection = seed();
        assert_eq!(pane_of(&connection, SESSION, CODER).unwrap(), None);
        set_pane(&connection, SESSION, CODER, "w8A:p2").unwrap();
        assert_eq!(
            pane_of(&connection, SESSION, CODER).unwrap().as_deref(),
            Some("w8A:p2")
        );
        assert!(pane_of(&connection, OTHER_SESSION, CODER).is_err());
        assert!(pane_of(&connection, SESSION, "ghost").is_err());
    }

    #[test]
    fn finds_the_session_orchestrator() {
        let connection = seed();
        assert_eq!(orchestrator_of(&connection, SESSION).unwrap(), ORCHESTRATOR);
        connection
            .execute(
                "UPDATE agent SET role = 'code.complex' WHERE session_id = ?1 AND id = ?2",
                (SESSION, ORCHESTRATOR),
            )
            .unwrap();
        assert_eq!(orchestrator_of(&connection, SESSION).unwrap(), ORCHESTRATOR);
        assert_eq!(
            orchestrator_of(&connection, OTHER_SESSION)
                .unwrap_err()
                .to_string(),
            format!("session {OTHER_SESSION} has no orchestrator")
        );
    }

    #[test]
    fn lists_live_children_and_summaries() {
        let mut connection = seed();
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(
            live_children(&connection, SESSION, ORCHESTRATOR).unwrap(),
            [(CODER.to_string(), "%2".to_string())]
        );
        clear_pane(&connection, SESSION, CODER).unwrap();
        assert!(
            live_children(&connection, SESSION, ORCHESTRATOR)
                .unwrap()
                .is_empty()
        );
        assert!(!answered(&connection, SESSION, CODER).unwrap());
        let root = temp_root("summary");
        let send = |connection: &mut Connection, sender, recipient, kind| {
            send_message(connection, &root, SESSION, sender, recipient, kind, "x").unwrap();
        };
        send(&mut connection, CODER, ORCHESTRATOR, "summary");
        send(&mut connection, ORCHESTRATOR, CODER, "ask");
        assert!(!answered(&connection, SESSION, CODER).unwrap());
        send(&mut connection, CODER, ORCHESTRATOR, "summary");
        assert!(answered(&connection, SESSION, CODER).unwrap());
    }
}
