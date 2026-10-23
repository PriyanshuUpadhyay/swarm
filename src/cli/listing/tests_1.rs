//! Tests for CLI listing.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn batch_keeps_a_listing_after_a_late_rering() {
        let root =
            std::env::temp_dir().join(format!("swarm-listing-rering-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        let ring_log = root.join("ring");
        let ring_path = swarm::adapter::shell_line(&[ring_log.to_string_lossy().into_owned()]);
        let gate = root.join("gate");
        let gate_path = swarm::adapter::shell_line(&[gate.to_string_lossy().into_owned()]);
        std::fs::write(root.join("adapters/fake.conf"),
            format!("self = true\nspawn = true\nring = while [ ! -e {gate_path} ]; do sleep 0.01; done; printf rung > {ring_path}\nlist = printf pane\nclose = true\ncapture = true\nscreen = printf '{{\"result\":{{\"agent\":{{\"agent_status\":\"idle\"}}}}}}'\n")
        ).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let session =
            swarm::store::create_session(&connection, "lane", &root, None, Some("fake")).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "chair").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "pane").unwrap();
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
            .execute(
                "UPDATE message SET created_at = unixepoch() - ?1",
                [RERING_UNSEEN_AFTER_SECS + 1],
            )
            .unwrap();
        let budget = std::time::Duration::from_secs(3);
        let started = std::time::Instant::now();
        // The ring ends only once the budget is spent, so it is late however fast the reads run.
        let opener = std::thread::spawn(move || {
            std::thread::sleep(budget);
            std::fs::write(gate, "").unwrap();
        });
        let listings = all_agent_listings(&mut connection, &root, budget).unwrap();
        opener.join().unwrap();
        assert!(started.elapsed() > budget);
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "rung");
        assert_eq!(listings[&session].agents.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_omits_slow_sessions_and_keeps_fast_sessions_within_budget() {
        let root =
            std::env::temp_dir().join(format!("swarm-batch-deadline-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let gate = root.join("gate");
        let slow_list = format!(
            "while [ ! -e {} ]; do sleep 0.01; done; printf pane",
            swarm::adapter::shell_line(&[gate.to_string_lossy().into_owned()])
        );
        let mut ids = Vec::new();
        for (index, (name, list, screen)) in [
            ("slow-list", slow_list.as_str(), "true"),
            ("slow-screen", "printf pane", "sleep 30"),
            ("fast", "printf pane", "true"),
        ]
        .into_iter()
        .enumerate()
        {
            std::fs::write(root.join("adapters").join(format!("{name}.conf")), format!(
                "self = true\nspawn = true\nring = true\nlist = {list}\nclose = true\ncapture = true\nscreen = {screen}\n"
            )).unwrap();
            let session =
                swarm::store::create_session(&connection, "lane", &root, None, Some(name)).unwrap();
            swarm::store::add_agent(&connection, &session, "coder", "code").unwrap();
            swarm::store::set_pane(&connection, &session, "coder", "pane").unwrap();
            // The hung sessions run first, so they cannot starve the healthy session behind them.
            connection
                .execute(
                    "UPDATE session SET created_at = ?1 WHERE id = ?2",
                    rusqlite::params![3 - index as i64, session],
                )
                .unwrap();
            ids.push(session);
        }
        let started = std::time::Instant::now();
        // The hung reads each spend their share, so the fast session gets about a third.
        let listings =
            all_agent_listings(&mut connection, &root, std::time::Duration::from_secs(9)).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        assert!(!listings.contains_key(&ids[0]));
        assert!(!listings.contains_key(&ids[1]));
        assert_eq!(listings[&ids[2]].agents.len(), 1);
        // The single-session list keeps its original unbounded list and bounded screen behavior.
        std::fs::write(&gate, "").unwrap();
        let adapter = swarm::adapter::load(&root, "slow-list").unwrap();
        assert!(list_agents(&mut connection, &root, &ids[0], &adapter).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_lists_the_panes_of_one_adapter_once() {
        let root =
            std::env::temp_dir().join(format!("swarm-batch-list-once-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        let calls = root.join("calls");
        let calls_path = swarm::adapter::shell_line(&[calls.to_string_lossy().into_owned()]);
        std::fs::write(root.join("adapters/fake.conf"), format!(
            "self = true\nspawn = true\nring = true\nlist = echo >> {calls_path}; printf pane\nclose = true\ncapture = true\nscreen = true\n"
        )).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let session =
                swarm::store::create_session(&connection, "lane", &root, None, Some("fake"))
                    .unwrap();
            swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
            swarm::store::set_pane(&connection, &session, CODER, "pane").unwrap();
            ids.push(session);
        }
        let listings =
            all_agent_listings(&mut connection, &root, std::time::Duration::from_secs(30)).unwrap();
        assert_eq!(std::fs::read_to_string(&calls).unwrap().lines().count(), 1);
        for id in &ids {
            assert_eq!(listings[id].agents.len(), 1);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_gives_its_budget_to_sessions_with_a_live_pane() {
        let root =
            std::env::temp_dir().join(format!("swarm-batch-live-share-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        std::fs::write(root.join("adapters/fake.conf"),
            "self = true\nspawn = true\nring = true\nlist = printf pane\nclose = true\ncapture = true\nscreen = sleep 1.5; printf '{\"result\":{\"agent\":{\"agent_status\":\"idle\"}}}'\n"
        ).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let mut live = String::new();
        // The live session lists first; an even split would give it 1/21 of the budget.
        for index in 0..21 {
            let session =
                swarm::store::create_session(&connection, "lane", &root, None, Some("fake"))
                    .unwrap();
            swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
            let pane = if index == 0 { "pane" } else { "closed" };
            swarm::store::set_pane(&connection, &session, CODER, pane).unwrap();
            connection
                .execute(
                    "UPDATE session SET created_at = ?1 WHERE id = ?2",
                    rusqlite::params![100 - index as i64, session],
                )
                .unwrap();
            if index == 0 {
                live = session;
            }
        }
        let listings =
            all_agent_listings(&mut connection, &root, std::time::Duration::from_secs(21)).unwrap();
        assert_eq!(listings.len(), 21);
        assert!(listings[&live].agents[0].alive == Some(true));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_listing_hides_the_kind_when_the_wait_is_stale() {
        let (root, mut connection, session) = ring_session(
            "listing-stale-kind",
            Some("claude"),
            "ring = true\nscreen = true",
            "",
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "waiting",
            "hook",
            None,
            Some("permission"),
            now - 50,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let output =
            serde_json::to_value(list_agents(&mut connection, &root, &session, &adapter).unwrap())
                .unwrap();
        let coder = output["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == CODER)
            .unwrap();
        assert!(coder["state"].is_null());
        assert!(coder["state_kind"].is_null());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_listing_clears_the_kind_when_an_idle_screen_ends_the_wait() {
        let (root, mut connection, session) = ring_session(
            "listing-idle-kind",
            Some("claude"),
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../../../tests/fixtures/screens/claude-idle.txt"),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "waiting",
            "hook",
            None,
            Some("permission"),
            now - 20,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let output =
            serde_json::to_value(list_agents(&mut connection, &root, &session, &adapter).unwrap())
                .unwrap();
        let coder = output["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == CODER)
            .unwrap();
        assert_eq!(coder["state"], "done");
        assert!(coder["state_kind"].is_null());
        let coder = swarm::store::agents(&connection, &session)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.state.as_deref(), Some("done"));
        assert_eq!(coder.state_kind, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_listing_uses_the_session_chair_provider_for_a_screen_gap() {
        let (root, mut connection, session) = ring_session(
            "listing-chair-gap",
            None,
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../../../tests/fixtures/screens/codex-failed.txt"),
        );
        swarm::store::set_chair(&connection, &session, Some(("codex", "chair"))).unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        swarm::store::set_state(
            &connection,
            &session,
            ORCHESTRATOR,
            "working",
            "hook",
            None,
            None,
            now - 20,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let output =
            serde_json::to_value(list_agents(&mut connection, &root, &session, &adapter).unwrap())
                .unwrap();
        let chair = output["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == ORCHESTRATOR)
            .unwrap();
        assert!(chair["provider"].is_null());
        assert_eq!(chair["state"], "failed");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The app kills `agents --json` at 20 s, so the listing types its re-ring and returns. A later
    /// pass settles the ring from the store and the screen, and only then tells the chair that it
    /// started no turn (ADR 0041, owner answer 2).
    #[test]
    fn the_listing_types_its_rering_and_a_later_pass_settles_it() {
        let (root, mut connection, session) = ring_session(
            "listing-rering",
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
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61, rings = 1",
                [],
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        let started = std::time::Instant::now();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(started.elapsed() < RING_TIMEOUT - std::time::Duration::from_secs(2));
        assert_eq!(delivery_of(&connection, &session, ask), None);
        assert!(chair_mail(&connection, &session).is_empty());

        // The ring's deadline passes with no turn started.
        connection
            .execute(
                "UPDATE message SET rung_at = rung_at - ?1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        let started = std::time::Instant::now();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(started.elapsed() < RING_TIMEOUT - std::time::Duration::from_secs(2));
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("unconfirmed")
        );
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("unconfirmed:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The app drops the listing's stderr, so a listing leaves the chair's own lost ring to
    /// `swarm sweep`, whose line names it (ADR 0041).
    #[test]
    fn a_listing_leaves_the_chairs_lost_ring_to_the_sweep_line() {
        let (root, mut connection, session) = ring_session(
            "chair-lost-listing",
            Some("agy"),
            "ring = true",
            include_str!("../../../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("agy", "chair-id"))).unwrap();
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
                "UPDATE message SET created_at = unixepoch() - ?1, rung_at = unixepoch() - ?1,
                                    rings = ?2",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(delivery_of(&connection, &session, summary), None);
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("unconfirmed {ORCHESTRATOR} {summary}")]
        );
        assert_eq!(
            delivery_of(&connection, &session, summary).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A listing ring whose `ring` verb fails stores nothing, like any listing ring, so the
    /// chair's own lost ring still reaches the `swarm sweep` line (ADR 0041).
    #[test]
    fn a_listing_ring_that_fails_leaves_the_chairs_lost_ring_to_the_sweep_line() {
        let (root, mut connection, session) = ring_session(
            "chair-ring-fails",
            Some("claude"),
            "ring = false\nscreen = cat '{screen}'",
            include_str!("../../../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "claude").unwrap();
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
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(delivery_of(&connection, &session, summary), None);

        // The failed ring's deadline passes.
        connection
            .execute(
                "UPDATE message SET rung_at = rung_at - ?1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("unconfirmed {ORCHESTRATOR} {summary}")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
