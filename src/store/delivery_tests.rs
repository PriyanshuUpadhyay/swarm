//! Message ring and stall tests.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn rings_only_unread_and_rerings_once_until_seen() {
        let mut connection = seed(0);
        let root = temp_root("old-unread");
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "old",
        )
        .unwrap();
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "new",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61 WHERE seq = 0",
                [],
            )
            .unwrap();

        assert!(!has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 62).unwrap());
        assert!(!rering_due(&connection, OTHER_SESSION, OUTSIDER, 60).unwrap());
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch(), rings = 1 WHERE seq = 0",
                [],
            )
            .unwrap();
        assert!(has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - 61 WHERE seq = 0",
                [],
            )
            .unwrap();
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection
            .execute("UPDATE message SET rings = 2 WHERE seq = 0", [])
            .unwrap();
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        inbox(&connection, SESSION, CODER).unwrap();
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        ack(&connection, SESSION, 0, CODER).unwrap();
        assert!(!has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
    }

    /// A sweep and a listing can both find a re-ring due before either one rings. The second
    /// update finds the messages rung a moment ago, so it rings nothing and the cap holds.
    #[test]
    fn two_passes_that_find_a_rering_due_ring_it_once() {
        let mut connection = seed(0);
        let root = temp_root("rering-race");
        let ask = send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1, delivery = 'unconfirmed'",
                [],
            )
            .unwrap();
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());

        let now = unix_now();
        assert_eq!(
            rering(&connection, SESSION, CODER, 60, now).unwrap(),
            [(ask, 2)]
        );
        assert_eq!(rering(&connection, SESSION, CODER, 60, now).unwrap(), []);
        assert_eq!(
            messages(&connection, SESSION, -1).unwrap()[0].delivery,
            None
        );
    }

    /// A pass that settles a ring stores its result only on that ring, once. A ring that another
    /// pass rang again since, or that has a result already, keeps what it has.
    #[test]
    fn a_ring_result_is_stored_only_on_the_ring_it_settles() {
        let mut connection = seed(0);
        let root = temp_root("delivery-race");
        let ask = send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        let first_ring = unix_now() - 61;
        connection
            .execute(
                "UPDATE message SET created_at = ?1, rung_at = ?1, rings = 1",
                [first_ring],
            )
            .unwrap();
        let delivery = |connection: &Connection| {
            messages(connection, SESSION, -1).unwrap()[0]
                .delivery
                .clone()
        };

        let second_ring = unix_now();
        rering(&connection, SESSION, CODER, 60, second_ring).unwrap();
        assert!(!set_delivery(&connection, SESSION, &[ask], first_ring, "unconfirmed").unwrap());
        assert_eq!(delivery(&connection), None);
        assert!(set_delivery(&connection, SESSION, &[ask], second_ring, "screen").unwrap());
        assert!(!set_delivery(&connection, SESSION, &[ask], second_ring, "unconfirmed").unwrap());
        assert_eq!(delivery(&connection).as_deref(), Some("screen"));
    }

    /// One ring's result is one write over every message it rang. Of two passes that settle a
    /// ring at the same time, exactly one stores the result and reports it, and the ring keeps
    /// one result.
    #[test]
    fn two_passes_settle_a_ring_of_many_messages_once() {
        let db = temp_root("settle-ring").join("swarm.db");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let connection = open(&db).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role)
                     VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'),
                            ('{CODER}', '{SESSION}', 'coder');
                 WITH RECURSIVE ring(seq) AS (SELECT 0 UNION ALL SELECT seq + 1 FROM ring
                                              WHERE seq < 499)
                 INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path,
                                      rung_at, rings)
                     SELECT '{SESSION}', seq, '{ORCHESTRATOR}', '{CODER}', 'ask',
                            'runs/' || seq || '.txt', 1700, 1
                     FROM ring;"
            ))
            .unwrap();
        // The race is timing, so it runs several times.
        for _ in 0..10 {
            connection
                .execute("UPDATE message SET delivery = NULL", [])
                .unwrap();
            let start = std::sync::Arc::new(std::sync::Barrier::new(2));
            let passes = ["hook", "unconfirmed"].map(|delivery| {
                let (db, start) = (db.clone(), start.clone());
                std::thread::spawn(move || {
                    let connection = open(&db).unwrap();
                    let seqs: Vec<i64> = (0..500).collect();
                    start.wait();
                    set_delivery(&connection, SESSION, &seqs, 1700, delivery).unwrap()
                })
            });
            let stored = passes.map(|pass| pass.join().unwrap());
            assert_eq!(stored.iter().filter(|stored| **stored).count(), 1);
            let results: Vec<Option<String>> = connection
                .prepare("SELECT DISTINCT delivery FROM message")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(results.len(), 1, "{results:?}");
            assert!(results[0].is_some());
        }
    }

    /// A ring's result is stored on the messages it rang, and a stall or lost-ring report is
    /// stored once per sender and kind (ADR 0041).
    #[test]
    fn stores_a_ring_result_and_each_report_once() {
        let mut connection = seed(0);
        let root = temp_root("delivery");
        let send = |connection: &mut Connection, kind: &str| {
            send_message(
                connection,
                &root,
                SESSION,
                CODER,
                ORCHESTRATOR,
                kind,
                "body",
            )
        };
        let ask = send(&mut connection, "ask").unwrap();
        let rung_at = unix_now();
        connection
            .execute("UPDATE message SET rung_at = ?1, rings = 1", [rung_at])
            .unwrap();
        assert_eq!(
            messages(&connection, SESSION, -1).unwrap()[0].delivery,
            None
        );
        assert!(set_delivery(&connection, SESSION, &[ask], rung_at, "maybe").is_err());
        set_delivery(&connection, SESSION, &[ask], rung_at, "hook").unwrap();
        assert_eq!(
            messages(&connection, SESSION, -1).unwrap()[0]
                .delivery
                .as_deref(),
            Some("hook")
        );
        send(&mut connection, "stall:unacked:0").unwrap();
        assert!(send(&mut connection, "stall:unacked:0").is_err());
        send(&mut connection, "unconfirmed:0").unwrap();
        assert!(send(&mut connection, "unconfirmed:0").is_err());
        send(&mut connection, "note").unwrap();
        send(&mut connection, "note").unwrap();
    }

    /// A child at done with a message it read and did not ack is stalled; so is a child at done
    /// that sent nothing after a ring that started its turn. A reported stall is not found again,
    /// and the chair is never the subject (ADR 0041).
    #[test]
    fn finds_each_stall_once() {
        let mut connection = seed(0);
        let root = temp_root("stalls");
        let run = |connection: &Connection, sql: &str| connection.execute_batch(sql).unwrap();
        let send = |connection: &mut Connection, from: &str, to: &str, kind: &str| {
            send_message(connection, &root, SESSION, from, to, kind, "body").unwrap()
        };
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        let ask = send(&mut connection, ORCHESTRATOR, CODER, "ask");
        run(
            &connection,
            "UPDATE message SET rung_at = unixepoch() - 30, rings = 1, delivery = 'hook'",
        );
        // Working, or done with no read message and a reply sent: no stall.
        set_state(
            &connection,
            SESSION,
            CODER,
            "working",
            "hook",
            None,
            None,
            0,
        )
        .unwrap();
        assert!(stalls(&connection, SESSION).unwrap().is_empty());

        // Read, then done without an ack.
        run(&connection, "UPDATE message SET seen_at = unixepoch() - 20");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        set_state(&connection, SESSION, CODER, "done", "hook", None, None, now).unwrap();
        assert_eq!(
            stalls(&connection, SESSION).unwrap(),
            [(CODER.to_string(), StallKind::Unacked, ask)]
        );
        send(
            &mut connection,
            CODER,
            ORCHESTRATOR,
            &format!("stall:unacked:{ask}"),
        );
        assert!(stalls(&connection, SESSION).unwrap().is_empty());

        // Acked, and its own report is no reply: it finished silent.
        ack(&connection, SESSION, ask, CODER).unwrap();
        assert_eq!(
            stalls(&connection, SESSION).unwrap(),
            [(CODER.to_string(), StallKind::Silent, ask)]
        );
        send(
            &mut connection,
            CODER,
            ORCHESTRATOR,
            &format!("stall:silent:{ask}"),
        );
        assert!(stalls(&connection, SESSION).unwrap().is_empty());

        // A reply after the ring is no stall; nor is the chair at done with unacked work.
        run(&connection, "DELETE FROM message WHERE kind GLOB 'stall:*'");
        send(&mut connection, CODER, ORCHESTRATOR, "summary");
        run(
            &connection,
            "UPDATE message SET seen_at = unixepoch() - 20 WHERE sender_id = 'coder'",
        );
        set_state(
            &connection,
            SESSION,
            ORCHESTRATOR,
            "done",
            "hook",
            None,
            None,
            now,
        )
        .unwrap();
        assert!(stalls(&connection, SESSION).unwrap().is_empty());
    }

    /// A host record is the agent's own report, as a hook is, so it proves a ring, and its `done`
    /// at the record's time counts for a stall.
    #[test]
    fn a_host_state_proves_a_turn_and_counts_for_a_stall() {
        let mut connection = seed(0);
        let root = temp_root("host-state");
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        let ask = send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "body",
        )
        .unwrap();
        connection
            .execute_batch(
                "UPDATE message SET rung_at = 100, rings = 1, delivery = 'hook', seen_at = 101",
            )
            .unwrap();
        set_state(
            &connection,
            SESSION,
            CODER,
            "working",
            "host",
            None,
            None,
            100,
        )
        .unwrap();
        assert!(turn_started(&connection, SESSION, CODER, 100).unwrap());
        assert!(!turn_started(&connection, SESSION, CODER, 101).unwrap());
        set_state(&connection, SESSION, CODER, "done", "host", None, None, 105).unwrap();
        assert_eq!(
            stalls(&connection, SESSION).unwrap(),
            [(CODER.to_string(), StallKind::Unacked, ask)]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
