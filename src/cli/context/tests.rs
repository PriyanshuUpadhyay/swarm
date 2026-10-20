//! Tests for CLI context.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn an_archived_codex_rollout_is_found_under_archived_sessions() {
        let codex_home =
            std::env::temp_dir().join(format!("swarm-archived-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&codex_home);
        let name = "rollout-2026-09-30T10-00-00-0199a000-0000-7000-8000-000000000001.jsonl";
        let recorded = codex_home.join("sessions/2026/09/30").join(name);
        let archived = codex_home.join("archived_sessions").join(name);
        std::fs::create_dir_all(archived.parent().unwrap()).unwrap();
        std::fs::write(&archived, "{}\n").unwrap();
        let recorded = recorded.to_string_lossy().into_owned();
        assert_eq!(
            resolved_agent_log(recorded.clone()),
            archived.to_string_lossy()
        );

        // A log still in place, or one that is nowhere, stays as recorded.
        std::fs::create_dir_all(codex_home.join("sessions/2026/09/30")).unwrap();
        std::fs::write(&recorded, "{}\n").unwrap();
        assert_eq!(resolved_agent_log(recorded.clone()), recorded);
        let gone = codex_home.join("sessions/2026/09/30/rollout-gone.jsonl");
        let gone = gone.to_string_lossy().into_owned();
        assert_eq!(resolved_agent_log(gone.clone()), gone);
        let _ = std::fs::remove_dir_all(&codex_home);
    }

    #[test]
    fn only_the_orchestrator_can_set_the_session_chair() {
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "code.complex").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();

        let error = set_chair_for_caller(&connection, &session, CODER, Some(("claude", "wrong")))
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "swarm: only the orchestrator can set the session chair"
        );
        set_chair_for_caller(
            &connection,
            &session,
            ORCHESTRATOR,
            Some(("claude", "right")),
        )
        .unwrap();
        let stored = swarm::store::sessions(&connection).unwrap();
        assert_eq!(stored[0].chair_id.as_deref(), Some("right"));
    }

    #[test]
    fn session_archive_can_be_listed_and_reversed() {
        let (home, connection, first) = stop_fixture("archive-list", None);
        let second = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        let active = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        let archived = session_command(&home, &["session", "archive", &first, &second]);
        assert!(archived.status.success());
        let normal = session_command(&home, &["sessions", "--json"]);
        assert!(normal.status.success());
        let normal: serde_json::Value = serde_json::from_slice(&normal.stdout).unwrap();
        assert_eq!(normal["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(normal["sessions"][0]["id"], active);
        assert!(normal["sessions"][0].get("archivedAt").is_none());
        let all = session_command(&home, &["sessions", "--json", "--archived"]);
        assert!(
            all.status.success(),
            "{}",
            String::from_utf8_lossy(&all.stderr)
        );
        let all: serde_json::Value = serde_json::from_slice(&all.stdout).unwrap();
        let rows = all["sessions"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        for id in [&first, &second] {
            let row = rows.iter().find(|row| row["id"] == *id).unwrap();
            let stored: i64 = connection
                .query_row(
                    "SELECT archived_at FROM session WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(stored > 0);
            assert_eq!(row["archivedAt"], stored);
        }
        let restored = session_command(&home, &["session", "unarchive", &first, &second]);
        assert!(
            restored.status.success(),
            "{}",
            String::from_utf8_lossy(&restored.stderr)
        );
        let normal = session_command(&home, &["sessions", "--json"]);
        let normal: serde_json::Value = serde_json::from_slice(&normal.stdout).unwrap();
        assert_eq!(normal["sessions"].as_array().unwrap().len(), 3);
        for row in normal["sessions"].as_array().unwrap() {
            assert!(row.get("archivedAt").is_none());
            let stored: Option<i64> = connection
                .query_row(
                    "SELECT archived_at FROM session WHERE id = ?1",
                    [row["id"].as_str().unwrap()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(stored, None);
        }
        assert!(
            session_command(&home, &["session", "unarchive", &first])
                .status
                .success()
        );
    }

    #[test]
    fn session_unarchive_names_unknown_id_and_rolls_back() {
        let (home, mut connection, known) = stop_fixture("unarchive-unknown", None);
        swarm::store::archive_sessions(&mut connection, std::slice::from_ref(&known)).unwrap();
        let unknown = uuid::Uuid::now_v7().to_string();
        let output = session_command(&home, &["session", "unarchive", &known, &unknown]);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains(&format!("swarm: no session {unknown}"))
        );
        let archived: Option<i64> = connection
            .query_row(
                "SELECT archived_at FROM session WHERE id = ?1",
                [&known],
                |row| row.get(0),
            )
            .unwrap();
        assert!(archived.is_some());
    }

    #[test]
    fn a_chair_pane_moves_to_the_new_session() {
        let root =
            std::env::temp_dir().join(format!("swarm-chair-move-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = printf '%s' '%1'\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let first = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/first"),
            None,
            None,
        )
        .unwrap();
        let second = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/second"),
            None,
            None,
        )
        .unwrap();

        add_agent(
            &connection,
            &root,
            "fake",
            &first,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap();
        add_agent(
            &connection,
            &root,
            "fake",
            &second,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap();

        assert_eq!(
            swarm::store::pane_of(&connection, &first, ORCHESTRATOR).unwrap(),
            None
        );
        assert_eq!(
            swarm::store::pane_of(&connection, &second, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%1")
        );
    }
}
