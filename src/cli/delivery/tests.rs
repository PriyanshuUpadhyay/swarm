//! Tests for CLI delivery.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    /// A ring whose Enter was lost sits in the input box. Swarm presses Enter again for each CLI
    /// whose box it can read, and the turn that starts proves the ring. AGY's box cannot be read,
    /// so it gets no second Enter.
    #[test]
    fn a_lost_enter_is_pressed_again_for_each_cli_whose_input_box_swarm_reads() {
        let claude_box = |text: &str| format!("────\n❯ {text}\n────\n");
        let runs = [
            ("claude", claude_box as fn(&str) -> String),
            ("codex", |text: &str| format!("• Done.\n\n› {text}\n\n  ? for shortcuts\n")),
            ("agy", |text: &str| format!("> {text}\n")),
        ]
        .map(|(provider, held)| {
            std::thread::spawn(move || {
                let (root, mut connection, session) = ring_session(
                    "lost-enter",
                    Some(provider),
                    "ring = echo ring >> \"{screen}.log\"; cp \"{screen}.held\" \"{screen}\"\n\
                     key = echo \"$SWARM_KEY\" >> \"{screen}.log\"; cp \"{screen}.working\" \"{screen}\"",
                    &held(""),
                );
                std::fs::write(root.join("screen.held"), held(&ring_text(&root))).unwrap();
                let working = match provider {
                    "claude" => format!("✻ Thinking…\n{}", claude_box("")),
                    _ => include_str!("../../../tests/fixtures/screens/codex-working.txt").to_string(),
                };
                std::fs::write(root.join("screen.working"), working).unwrap();
                let seq = send_task(&root, &mut connection, &session);
                let log = std::fs::read_to_string(root.join("screen.log")).unwrap();
                let delivery = delivery_of(&connection, &session, seq);
                std::fs::remove_dir_all(root).unwrap();
                (provider, log, delivery)
            })
        });
        let results: Vec<_> = runs.map(|run| run.join().unwrap()).into();
        assert_eq!(
            results,
            [
                (
                    "claude",
                    "ring\nEnter\n".to_string(),
                    Some("screen".to_string())
                ),
                (
                    "codex",
                    "ring\nEnter\n".to_string(),
                    Some("screen".to_string())
                ),
                ("agy", "ring\n".to_string(), Some("unconfirmed".to_string())),
            ]
        );
    }

    /// A report of a lost message that a pass cannot send, here because the chair has no pane yet,
    /// is sent by a later pass. Its message is past its last ring, so no ring would find it again.
    #[test]
    fn a_lost_message_report_that_failed_to_send_is_sent_by_a_later_pass() {
        let (root, mut connection, session) = ring_session(
            "lost-retry",
            Some("agy"),
            "ring = true",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
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
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 2, delivery = 'unconfirmed'",
                [],
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("unconfirmed {CODER} {ask}")]
        );
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("unconfirmed:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A report rings the chair, and its Enter would answer a question on the chair's screen. So
    /// the listing and the sweep send a report only while the chair's screen reads idle, and a
    /// later pass sends it. A chair that `session new` registered has no provider on its row, so
    /// both read its screen with the session's chair provider.
    #[test]
    fn a_report_waits_until_the_chairs_screen_reads_idle() {
        let (root, mut connection, session) = ring_session(
            "report-idle",
            Some("claude"),
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../../../tests/fixtures/screens/claude-question.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        // `session new` in a Claude pane names the chair only on the session, not on its row.
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
        connection
            .execute_batch(
                "UPDATE message SET delivery = 'hook', rings = 1, rung_at = unixepoch() - 30,
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

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        assert!(chair_mail(&connection, &session).is_empty());

        let show = |screen: &str| std::fs::write(root.join("screen"), screen).unwrap();
        show(include_str!(
            "../../../tests/fixtures/screens/claude-idle.txt"
        ));
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("stall:unacked:{ask}"))]
        );

        swarm::store::ack(&connection, &session, ask, CODER).unwrap();
        show(include_str!(
            "../../../tests/fixtures/screens/claude-question.txt"
        ));
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        assert_eq!(chair_mail(&connection, &session).len(), 1);
        show(include_str!(
            "../../../tests/fixtures/screens/claude-idle.txt"
        ));
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("stall coder silent {ask}")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_routing_role_chair_receives_a_child_finish() {
        let root =
            std::env::temp_dir().join(format!("swarm-routing-chair-test-{}", std::process::id()));
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
            Some("fake"),
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "code.complex").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%9").unwrap();

        let chair = swarm::store::orchestrator_of(&connection, &session).unwrap();
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            CODER,
            &chair,
            "summary",
            "done",
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(ring_log).unwrap(),
            format!("%9:{}\n", ring_text(&root))
        );
    }

    #[test]
    fn deliver_to_agent_without_pane_writes_no_message() {
        let root = std::env::temp_dir().join(format!("swarm-no-pane-test-{}", std::process::id()));
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

        let error = deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap_err()
        .to_string();

        assert_eq!(
            error,
            "swarm: orchestrator has no pane; nothing would ring it"
        );
        assert_eq!(
            swarm::store::messages(&connection, &session, -1)
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn deliver_from_or_to_an_unknown_agent_writes_no_message() {
        let root =
            std::env::temp_dir().join(format!("swarm-unknown-agent-test-{}", std::process::id()));
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
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();

        for (sender, recipient) in [("ghost", ORCHESTRATOR), (ORCHESTRATOR, "ghost")] {
            let error = deliver(
                &mut connection,
                &root,
                "fake",
                &session,
                sender,
                recipient,
                "ask",
                "hi",
            )
            .unwrap_err()
            .to_string();
            assert_eq!(
                error,
                format!("swarm: ghost is not an agent in session {session}")
            );
        }
        assert_eq!(
            swarm::store::messages(&connection, &session, -1)
                .unwrap()
                .len(),
            0
        );
    }
}
