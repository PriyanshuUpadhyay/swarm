//! Tests for CLI ring.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    /// A store error while reading a ring's proof proves nothing either way. The waiting ring and
    /// a late pass store no result, so the chair is not told that a delivered message was lost.
    #[test]
    fn a_store_error_in_the_proof_check_stores_no_ring_result() {
        let (root, mut connection, session) = ring_session(
            "proof-store-error",
            Some("claude"),
            "ring = true",
            include_str!("../../../tests/fixtures/screens/claude-idle.txt"),
        );
        let seq = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        let rung_at = unix_now().unwrap();
        connection
            .execute("UPDATE message SET rung_at = ?1, rings = 1", [rung_at])
            .unwrap();
        // A temp `agent` with no `state_source` shadows the real table, so the hook read fails
        // and the ring's other agent reads still work.
        connection
            .execute_batch(
                "CREATE TEMP TABLE agent AS SELECT * FROM main.agent;
                 ALTER TABLE temp.agent DROP COLUMN state_source;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let delivery = ring_and_record(
            &connection,
            &root,
            Ok(adapter.clone()),
            &session,
            CODER,
            "%2",
            &[seq],
            rung_at,
            Proof::Wait,
        );
        assert_eq!(delivery, None);
        assert_eq!(delivery_of(&connection, &session, seq), None);

        connection
            .execute(
                "UPDATE message SET rung_at = ?1",
                [rung_at - RING_TIMEOUT.as_secs() as i64 - 1],
            )
            .unwrap();
        assert!(
            settle_rings(
                &mut connection,
                &root,
                &adapter,
                &session,
                Proof::Wait,
                &mut Vec::new()
            )
            .is_err()
        );
        assert_eq!(delivery_of(&connection, &session, seq), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Two sweeps can read the same unsettled ring of the chair. Only the pass whose write stores
    /// the result prints the chair's line, so the line comes once.
    #[test]
    fn a_pass_that_lost_the_race_to_settle_the_chairs_ring_prints_no_line() {
        let (root, mut connection, session) = ring_session(
            "settle-race",
            Some("agy"),
            "ring = true",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = ?2",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        // The other pass stored its result first, so this pass's guarded write matches no row.
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER other_pass BEFORE UPDATE OF delivery ON main.message
                 BEGIN SELECT RAISE(IGNORE); END;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = Vec::new();
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Wait,
            &mut lines,
        )
        .unwrap();
        assert!(lines.is_empty(), "{lines:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A store error on a later ring fails the sweep pass, but the chair's own lost-ring line that
    /// the pass already stored is still printed: no later pass finds that ring again.
    #[test]
    fn a_failed_settle_still_prints_the_chairs_lost_ring_it_stored() {
        let (root, mut connection, session) = ring_session(
            "settle-error",
            Some("agy"),
            "ring = true",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        swarm::store::add_agent(&connection, &session, "reviewer", "reviewer").unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            "reviewer",
            "ask",
            "review",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = ?2",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        // The reviewer's ring settles after the chair's, and its write fails.
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER store_error BEFORE UPDATE OF delivery ON main.message
                 WHEN NEW.recipient_id = 'reviewer'
                 BEGIN SELECT RAISE(ABORT, 'store error'); END;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = Vec::new();
        let pass = sweep_once(
            &mut connection,
            &root,
            &adapter,
            &session,
            ORCHESTRATOR,
            &mut lines,
        );
        assert!(pass.is_err());
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} {summary}")]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A store error while the sweep re-rings the chair comes before the ring, so the ring is
    /// left for the next pass, whose line names the chair's lost message.
    #[test]
    fn a_failed_chair_read_in_a_rering_leaves_the_lost_line_to_the_next_pass() {
        let (root, mut connection, session) = ring_session(
            "rering-error",
            Some("agy"),
            "ring = false",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1",
                [],
            )
            .unwrap();
        // A temp `agent` with no `role` shadows the real table, so only the chair read fails.
        connection
            .execute_batch(
                "CREATE TEMP TABLE agent AS SELECT * FROM main.agent;
                 ALTER TABLE temp.agent DROP COLUMN role;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = Vec::new();
        let pass = sweep_once(
            &mut connection,
            &root,
            &adapter,
            &session,
            ORCHESTRATOR,
            &mut lines,
        );
        assert!(pass.is_err());
        connection.execute_batch("DROP TABLE temp.agent").unwrap();
        lines.extend(swept(&mut connection, &root, &adapter, &session));
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} {summary}")]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A re-ring of the chair whose result write fails stores nothing, so it prints no line. The
    /// later pass that stores the result prints the chair's line, so the line comes once.
    #[test]
    fn a_rering_whose_result_was_not_stored_leaves_the_lost_line_to_the_pass_that_stores_it() {
        let (root, mut connection, session) = ring_session(
            "rering-unstored",
            Some("agy"),
            "ring = false",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1",
                [],
            )
            .unwrap();
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER store_error BEFORE UPDATE OF delivery ON main.message
                 WHEN NEW.delivery IS NOT NULL
                 BEGIN SELECT RAISE(ABORT, 'store error'); END;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = swept(&mut connection, &root, &adapter, &session);
        connection
            .execute_batch("DROP TRIGGER temp.store_error")
            .unwrap();
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        lines.extend(swept(&mut connection, &root, &adapter, &session));
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} {summary}")]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A report whose row is stored but whose ring mark fails is sent: the pass keeps its line,
    /// a later pass does not send it twice, and the sweep's re-ring rings the unrung row.
    #[test]
    fn a_stored_report_whose_ring_mark_failed_is_sent_and_rung_later() {
        let (root, mut connection, session) =
            ring_session("report-unrung", None, "ring = true", "anything\n");
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER store_error BEFORE UPDATE OF rung_at ON main.message
                 BEGIN SELECT RAISE(ABORT, 'store error'); END;",
            )
            .unwrap();
        let send = |connection: &mut rusqlite::Connection| {
            report(
                connection,
                &root,
                "fake",
                &session,
                CODER,
                "stall:silent:0",
                "coder sent nothing back",
                Proof::Wait,
            )
        };
        assert!(send(&mut connection));
        assert!(!send(&mut connection));
        assert_eq!(chair_mail(&connection, &session).len(), 1);

        connection
            .execute_batch(
                "DROP TRIGGER temp.store_error;
                 UPDATE message SET created_at = unixepoch() - 61;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        swept(&mut connection, &root, &adapter, &session);
        let rings: i64 = connection
            .query_row("SELECT rings FROM message", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rings, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A child that reads and acks its message and ends its turn between two listing passes leaves
    /// its hook at done and its screen idle, so neither proves the ring. The read proves it: the
    /// pass stores `seen`, sends no `unconfirmed` report, and counts the ring for the silent stall
    /// (ADR 0041, owner answer 3).
    #[test]
    fn a_ring_whose_message_was_read_is_seen_though_its_turn_has_ended() {
        let (root, mut connection, session) = ring_session(
            "seen",
            Some("claude"),
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../../../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "claude").unwrap();
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
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - ?1, rung_at = unixepoch() - ?1,
                                    rings = ?2, seen_at = unixepoch() - 5",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        swarm::store::ack(&connection, &session, ask, CODER).unwrap();
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
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("seen")
        );
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("stall:silent:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// After the second ring of a message starts no turn, the chair gets one `unconfirmed`
    /// message from the agent, not silence. A lost ring to the chair itself is only a sweep line,
    /// because a message would ring the same lost pane a third time (ADR 0041).
    #[test]
    fn the_chair_hears_of_a_message_whose_second_ring_started_no_turn() {
        let (root, mut connection, session) = ring_session(
            "lost-ring",
            Some("agy"),
            "ring = true",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        // The chair's screen reads idle, so a report may ring it.
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        send_task(&root, &mut connection, &session);
        let backdate =
            "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61
             WHERE recipient_id = 'coder'";
        connection.execute(backdate, []).unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        let lines = swept(&mut connection, &root, &adapter, &session);
        assert_eq!(lines, ["unconfirmed coder 0"]);
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), "unconfirmed:0".to_string())]
        );
        let body = std::fs::read_to_string(root.join(format!("runs/{session}/1.txt"))).unwrap();
        assert_eq!(
            body,
            "message 0 to coder was not delivered: no turn started after 2 rings"
        );
        connection.execute(backdate, []).unwrap();
        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        assert_eq!(chair_mail(&connection, &session).len(), 1);

        // The chair's own lost message: a line, and no message to itself.
        connection.execute("DELETE FROM message", []).unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%2").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("agy", "chair-id"))).unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute("UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61, rings = 1", [])
            .unwrap();
        swarm::store::clear_pane(&connection, &session, CODER).unwrap();
        let lines = swept(&mut connection, &root, &adapter, &session);
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} 0")]);
        assert_eq!(chair_mail(&connection, &session).len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn orchestrator_agent_add_records_self_pane_and_child_finish_rings_it() {
        let root = std::env::temp_dir().join(format!("swarm-chair-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let ring_log = root.join("rings");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            format!(
                "self = printf '%s' '%9'\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
                ring_log.display()
            ),
        )
        .unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();

        add_agent(
            &connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();

        assert_eq!(
            swarm::store::pane_of(&connection, &session, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%9")
        );
        assert_eq!(
            std::fs::read_to_string(ring_log).unwrap(),
            format!("%9:{}\n", ring_text(&root))
        );
    }
}
