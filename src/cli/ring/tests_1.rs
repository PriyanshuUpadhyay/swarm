//! Tests for CLI ring.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn rering_finishes_after_the_session_deadline() {
        patient_rings();
        let root =
            std::env::temp_dir().join(format!("swarm-ring-deadline-{}", uuid::Uuid::now_v7()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "chair").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "one",
        )
        .unwrap();
        connection
            .execute("UPDATE message SET created_at = unixepoch() - 61", [])
            .unwrap();
        let mut adapter = swarm::adapter::parse("fake", "self = true\nspawn = true\nring = printf text >> \"$SWARM_PANE\"; sleep 0.3; printf enter >> \"$SWARM_PANE\"\nlist = true\nclose = true\ncapture = true\n").unwrap();
        adapter.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
        let ring_log = root.join("ring");
        rering_if_due(
            &mut connection,
            &root,
            &adapter,
            &session,
            CODER,
            ring_log.to_str().unwrap(),
            Proof::Wait,
        )
        .unwrap();
        let rings: i64 = connection
            .query_row("SELECT rings FROM message", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rings, 1);
        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), "textenter");
        assert!(adapter.check_deadline().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_hung_ring_ends_at_the_ring_timeout() {
        let (root, mut connection, session) =
            ring_session("hung-ring", Some("codex"), "ring = sleep 60", "› \n");
        let started = std::time::Instant::now();
        let seq = send_task(&root, &mut connection, &session);
        assert!(started.elapsed() < RING_TIMEOUT + std::time::Duration::from_secs(2));
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_rering_presses_enter_after_the_session_deadline() {
        let root = std::env::temp_dir().join(format!("swarm-claude-ring-{}", uuid::Uuid::now_v7()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "chair").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
        swarm::store::set_provider(&connection, &session, CODER, "claude").unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "one",
        )
        .unwrap();
        connection
            .execute("UPDATE message SET created_at = unixepoch() - 61", [])
            .unwrap();
        let screen = root.join("screen");
        let log = root.join("log");
        std::fs::write(&screen, "────\n❯ \n────\n").unwrap();
        // The ring stays typed in the composer until a key clears it.
        let mut adapter = swarm::adapter::parse(
            "fake",
            &format!(
                "self = true\nspawn = true\nlist = true\nclose = true\n\
                 capture = cat \"$SWARM_PANE\"\n\
                 ring = echo ring >> '{log}'; printf '────\\n❯ %s\\n────\\n' \"$SWARM_TEXT\" > \"$SWARM_PANE\"\n\
                 key = echo \"$SWARM_KEY\" >> '{log}'; printf '────\\n❯ \\n────\\n' > \"$SWARM_PANE\"\n",
                log = log.display()
            ),
        )
        .unwrap();
        adapter.deadline = Some(std::time::Instant::now());
        rering_if_due(
            &mut connection,
            &root,
            &adapter,
            &session,
            CODER,
            screen.to_str().unwrap(),
            Proof::Wait,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "ring\nEnter\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A fake Claude pane that draws its input box on the third read, keeps a ring unsent in the
    /// box as a starting Claude does, and starts a turn on Enter.
    #[test]
    fn a_ring_to_claude_waits_for_the_input_box_and_presses_enter_until_it_sends() {
        patient_rings();
        let (dir, connection, session) =
            ring_session("ring-pane", Some("claude"), "", "$ claude\n");
        let box_rows = |text: &str| format!("────\n❯ {text}\n────\n");
        std::fs::write(dir.join("empty"), box_rows("")).unwrap();
        std::fs::write(
            dir.join("working"),
            format!("✻ Thinking…\n{}", box_rows("")),
        )
        .unwrap();
        let d = dir.display();
        let adapter = swarm::adapter::parse(
            "fake",
            &format!(
                "self = true\nspawn = true\nlist = true\nclose = true\n\
                 capture = n=$(($(cat '{d}/reads' 2>/dev/null || echo 0) + 1)); echo $n > '{d}/reads'; \
                 [ $n = 3 ] && cp '{d}/empty' '{d}/screen'; echo read >> '{d}/log'; cat '{d}/screen'\n\
                 ring = echo ring >> '{d}/log'; printf '────\\n❯ %s\\n────\\n' \"$SWARM_TEXT\" > '{d}/screen'\n\
                 key = echo \"$SWARM_KEY\" >> '{d}/log'; cp '{d}/working' '{d}/screen'\n"
            ),
        )
        .unwrap();
        let delivery = ring_pane(
            &connection,
            &adapter,
            &dir,
            &session,
            CODER,
            "%2",
            Proof::Wait,
        )
        .unwrap();
        assert_eq!(delivery, Some(Delivery::Screen));
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "read\nread\nread\nring\nread\nEnter\nread\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deliver_rings_only_when_no_unread_is_rung() {
        patient_rings();
        let root = std::env::temp_dir().join(format!("swarm-deliver-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            format!(
                "self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
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
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();

        let first = deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "first",
        )
        .unwrap();
        let second = deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "second",
        )
        .unwrap();
        assert_eq!((first, second), (0, 1));
        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);

        ack(&mut connection, &root, "fake", &session, CODER, first).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
        ack(&mut connection, &root, "fake", &session, CODER, second).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "third",
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(3));

        // The coder listed "third" but has not acked it, so "fourth" is news and must ring.
        swarm::store::inbox(&connection, &session, CODER).unwrap();
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "fourth",
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(4));
    }

    /// A ring that starts no turn is stored as unconfirmed at its deadline, for each provider, and
    /// the send still succeeds (ADR 0041).
    #[test]
    fn a_ring_that_starts_no_turn_is_unconfirmed_at_its_deadline() {
        let runs = [
            (
                "claude",
                include_str!("../../../tests/fixtures/screens/claude-idle.txt"),
            ),
            (
                "codex",
                include_str!("../../../tests/fixtures/screens/codex-idle.txt"),
            ),
            (
                "agy",
                include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
            ),
        ]
        .map(|(provider, screen)| {
            std::thread::spawn(move || {
                let (root, mut connection, session) = ring_session(
                    "unconfirmed",
                    Some(provider),
                    "ring = true\nkey = true",
                    screen,
                );
                let started = std::time::Instant::now();
                let seq = send_task(&root, &mut connection, &session);
                assert!(
                    started.elapsed() >= RING_TIMEOUT - std::time::Duration::from_secs(1),
                    "{provider}"
                );
                assert_eq!(
                    delivery_of(&connection, &session, seq).as_deref(),
                    Some("unconfirmed"),
                    "{provider}"
                );
                std::fs::remove_dir_all(root).unwrap();
            })
        });
        for run in runs {
            run.join().unwrap();
        }
    }

    /// A working screen after the ring proves it. A pane with no provider and no Herdr status
    /// cannot prove a ring, so it is stored as unchecked with no wait.
    #[test]
    fn a_working_screen_proves_a_ring_and_a_pane_with_no_reader_is_unchecked() {
        patient_rings();
        let (root, mut connection, session) = ring_session(
            "screen-proof",
            Some("agy"),
            "ring = cp \"{screen}.working\" \"{screen}\"",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        std::fs::write(
            root.join("screen.working"),
            include_str!("../../../tests/fixtures/screens/agy-working.txt"),
        )
        .unwrap();
        let seq = send_task(&root, &mut connection, &session);
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("screen")
        );
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut connection, session) =
            ring_session("unchecked", None, "ring = true", "anything\n");
        let started = std::time::Instant::now();
        let seq = send_task(&root, &mut connection, &session);
        // Well before the 30 s deadline of a patient ring.
        assert!(started.elapsed() < std::time::Duration::from_secs(15));
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("unchecked")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A failed read is no proof yet. A screen verb that fails does not make a pane with no
    /// provider unchecked, and a capture that keeps failing still lets the deadline settle a ring.
    #[test]
    fn a_failed_read_proves_nothing_and_the_ring_still_meets_its_deadline() {
        let (root, mut connection, session) = ring_session(
            "screen-fails",
            None,
            "ring = true\nscreen = false",
            "anything\n",
        );
        let seq = send_task(&root, &mut connection, &session);
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut connection, session) =
            ring_session("capture-fails", Some("claude"), "ring = true", "");
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
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = 1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        std::fs::remove_file(root.join("screen")).unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Wait,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A pass whose batch deadline is spent cannot read the pane. That is no evidence, so a late
    /// ring with no proof in the store stays for a pass with time to read its screen.
    #[test]
    fn a_pass_out_of_time_leaves_a_late_ring_unsettled() {
        let (root, mut connection, session) = ring_session(
            "settle-no-time",
            Some("agy"),
            "ring = true",
            include_str!("../../../tests/fixtures/screens/agy-working.txt"),
        );
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
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = 1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        let adapter = swarm::adapter::Adapter {
            deadline: Some(std::time::Instant::now()),
            ..swarm::adapter::load(&root, "fake").unwrap()
        };
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Later,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(delivery_of(&connection, &session, ask), None);

        // A pass with time reads the working screen that a busy CLI shows (ADR 0038).
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Later,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("screen")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
