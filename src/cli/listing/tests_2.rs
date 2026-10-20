//! Tests for CLI listing.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn batch_failed_pane_lists_do_not_share_the_live_sessions_budget() {
        let root = std::env::temp_dir().join(format!(
            "swarm-batch-failed-list-share-{}",
            uuid::Uuid::now_v7()
        ));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        for (name, list, screen) in [
            (
                "live",
                "printf pane",
                "sleep 0.2; printf '{\"result\":{\"agent\":{\"agent_status\":\"idle\"}}}'",
            ),
            ("failed", "false", "true"),
        ] {
            std::fs::write(
                root.join("adapters").join(format!("{name}.conf")),
                format!(
                    "self = true\nspawn = true\nring = true\nlist = {list}\nclose = true\ncapture = true\nscreen = {screen}\n"
                ),
            )
            .unwrap();
        }
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let mut ids = Vec::new();
        // The live session runs first and needs more than an even share of the budget.
        for index in 0..50 {
            let adapter = if index == 0 { "live" } else { "failed" };
            let session =
                swarm::store::create_session(&connection, "lane", &root, None, Some(adapter))
                    .unwrap();
            swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
            swarm::store::set_pane(&connection, &session, CODER, "pane").unwrap();
            connection
                .execute(
                    "UPDATE session SET created_at = ?1 WHERE id = ?2",
                    rusqlite::params![100 - index, session],
                )
                .unwrap();
            ids.push(session);
        }
        let listings = all_agent_listings(
            &mut connection,
            &root,
            std::time::Duration::from_millis(900),
        )
        .unwrap();
        let listed = listings
            .get(&ids[0])
            .expect("live session must not time out");
        assert_eq!(listed.agents[0].alive, Some(true));
        assert_eq!(listings.len(), ids.len());
        for id in &ids[1..] {
            assert_eq!(listings[id].agents[0].alive, None);
            assert!(listings[id].agents[0].prompt.is_none());
        }
        // A single-session listing still falls back to unknown liveness on adapter failure.
        let adapter = swarm::adapter::load(&root, "failed").unwrap();
        let listed = list_agents(&mut connection, &root, &ids[1], &adapter).unwrap();
        assert_eq!(listed.agents[0].alive, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A listing reads every unarchived session, and a restarted tmux or Herdr server can give a
    /// stored pane id to another pane. So the listing sends a report only while the adapter lists
    /// the chair's pane.
    #[test]
    fn a_listing_sends_no_report_while_the_chairs_pane_is_not_listed() {
        let (root, mut connection, session) = ring_session(
            "stall-listing",
            None,
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../../../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("claude", "chair-id"))).unwrap();
        let ask = send_task(&root, &mut connection, &session);
        connection
            .execute_batch(
                "UPDATE message SET delivery = 'hook', rung_at = unixepoch() - 30,
                                    seen_at = unixepoch() - 20",
            )
            .unwrap();
        let now = unix_now().unwrap();
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "done",
            "hook",
            None,
            None,
            now,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%9").unwrap();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(chair_mail(&connection, &session).is_empty());
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("stall:unacked:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The listing reads the screens first, then rings and settles, which takes seconds. A
    /// question that comes up on the chair's screen in that time stops the report, because the
    /// listing reads the chair's screen again right before it.
    #[test]
    fn a_listing_reads_the_chairs_screen_again_before_a_report() {
        // The coder's re-ring stands in for the gap: the fake ring puts a question on the screen
        // that the chair's pane shows too.
        let (root, mut connection, session) = ring_session(
            "report-fresh-idle",
            Some("claude"),
            "ring = cp '{screen}.question' '{screen}'\nscreen = cat '{screen}'",
            include_str!("../../../tests/fixtures/screens/claude-idle.txt"),
        );
        std::fs::write(
            root.join("screen.question"),
            include_str!("../../../tests/fixtures/screens/claude-question.txt"),
        )
        .unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("claude", "chair-id"))).unwrap();
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        let follow_up = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "more",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET delivery = 'hook', rings = 1, rung_at = unixepoch() - 30,
                                    seen_at = unixepoch() - 20
                 WHERE seq = ?1",
                [ask],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1
                 WHERE seq = ?1",
                [follow_up],
            )
            .unwrap();
        let now = unix_now().unwrap();
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "done",
            "hook",
            None,
            None,
            now,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(
            chair_mail(&connection, &session).is_empty(),
            "{:?}",
            chair_mail(&connection, &session)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_archived_listing_keeps_active_rows_and_limits_archives_in_archive_order() {
        let (home, connection, first_active) = stop_fixture("archive-limit", None);
        let second_active =
            swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        let mut archived_ids = Vec::new();
        for index in 0..55 {
            let id = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
            connection
                .execute(
                    "UPDATE session SET archived_at = ?1, created_at = ?2 WHERE id = ?3",
                    (10000 + index, 20000 - index, &id),
                )
                .unwrap();
            archived_ids.push(id);
        }
        let all = session_command(&home, &["sessions", "--json", "--archived"]);
        assert!(
            all.status.success(),
            "{}",
            String::from_utf8_lossy(&all.stderr)
        );
        let all: serde_json::Value = serde_json::from_slice(&all.stdout).unwrap();
        let rows = all["sessions"].as_array().unwrap();
        assert_eq!(rows.len(), 52);
        assert!(rows[..2].iter().all(|row| row.get("archivedAt").is_none()));
        assert!(rows[..2].iter().any(|row| row["id"] == first_active));
        assert!(rows[..2].iter().any(|row| row["id"] == second_active));
        for (row, id) in rows[2..].iter().zip(archived_ids[5..].iter().rev()) {
            assert_eq!(row["id"], *id);
        }
        assert_eq!(rows[2]["archivedAt"], 10054);
        assert_eq!(rows.last().unwrap()["archivedAt"], 10005);
        drop(connection);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn launch_fields_survive_a_fresh_connection_and_listing() {
        let root = std::env::temp_dir().join(format!("swarm-runner-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("bus.sqlite");
        let connection = swarm::store::open(&db).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        let adapter = swarm::adapter::parse("fake", "self = true\nspawn = printf '%s' '%2'\nring = true\nlist = true\nclose = true\ncapture = true\n").unwrap();
        let runner = swarm::config::Runner {
            provider: Provider::Codex,
            model: "gpt-6".into(),
            effort: "high".into(),
            sandbox: None,
            approval: None,
            permission: None,
        };
        register_spawned_pane(
            &connection,
            &adapter,
            &session,
            CODER,
            "code.complex",
            Some("codex"),
            &[],
            Some((&runner, Some("work"))),
            || Ok(None),
        )
        .unwrap();
        let profile: String = connection
            .query_row("SELECT profile FROM agent WHERE id = ?1", [CODER], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(profile, "code.complex");
        drop(connection);
        let mut fresh = swarm::store::open(&db).unwrap();
        let output =
            serde_json::to_value(list_agents(&mut fresh, &root, &session, &adapter).unwrap())
                .unwrap();
        let agent = &output["agents"][0];
        assert_eq!(agent["profile"], "code.complex");
        assert_eq!(agent["runner"], "codex/gpt-6/high");
        assert_eq!(agent["model"], "gpt-6");
        assert_eq!(agent["effort"], "high");
        assert_eq!(agent["account"], "work");
        assert!(agent.get("costUsd").is_none());
        assert!(agent.get("tokens").is_none());
    }
}
