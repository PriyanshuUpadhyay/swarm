//! Tests for CLI spawn.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn a_run_script_refuses_an_agent_id_that_leaves_runs() {
        let root =
            std::env::temp_dir().join(format!("swarm-script-escape-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let session = "0199a000-0000-7000-8000-000000000001";
        for bad in ["../../x", "../x", "a/b", "X", ""] {
            assert!(script_line(&root, session, bad, "true").is_err(), "{bad}");
        }
        assert!(script_line(&root, "../session", "coder", "true").is_err());
        assert!(!root.join("x.sh").exists() && !root.parent().unwrap().join("x.sh").exists());
        assert!(!root.join("runs").exists());
        let line = script_line(&root, session, "coder", "true").unwrap();
        assert!(
            line.ends_with(&format!("runs/{session}/coder.sh'")),
            "{line}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_long_command_is_rung_as_a_short_line_that_sources_it() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-script-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let session = "0199a000-0000-7000-8000-000000000001";
        let command = format!("'claude' '--settings' '{}'", "x".repeat(2000));

        let line = script_line(&root, session, CODER, &command).unwrap();
        let script = root.join(format!("runs/{session}/{CODER}.sh"));
        assert_eq!(line, format!(". '{}'", script.display()));
        assert!(line.len() < 256);
        assert_eq!(
            std::fs::read_to_string(&script).unwrap(),
            format!("{command}\n")
        );
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&script), 0o600);
        assert_eq!(mode(script.parent().unwrap()), 0o700);

        std::fs::write(&script, "echo ran\n").unwrap();
        let shell = std::process::Command::new("sh")
            .args(["-c", &format!("{line}; echo after")])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&shell.stdout), "ran\nafter\n");
    }

    #[test]
    fn a_failed_adapter_spawn_rolls_back_the_agent() {
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        let failed = swarm::adapter::parse(
            "failed",
            "self = true\nspawn = echo refused >&2; exit 1\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();

        assert!(
            register_spawned_pane(
                &connection,
                &failed,
                &session,
                CODER,
                "coder",
                None,
                &[],
                None,
                || Ok(None),
            )
            .is_err()
        );
        assert!(
            swarm::store::agents(&connection, &session)
                .unwrap()
                .is_empty()
        );

        let working = swarm::adapter::parse(
            "working",
            "self = true\nspawn = printf '%s' '%2'\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        assert_eq!(
            register_spawned_pane(
                &connection,
                &working,
                &session,
                CODER,
                "coder",
                None,
                &[],
                None,
                || Ok(None),
            )
            .unwrap()
            .0,
            "%2"
        );
    }

    #[test]
    fn orchestrator_registration_sets_the_session_adapter() {
        let root =
            std::env::temp_dir().join(format!("swarm-chair-adapter-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = printf '%s' '%1'\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            Some("tmux"),
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

        let stored = swarm::store::sessions(&connection).unwrap();
        assert_eq!(stored[0].adapter.as_deref(), Some("fake"));
    }

    #[test]
    fn refused_orchestrator_registration_leaves_no_agent() {
        let root =
            std::env::temp_dir().join(format!("swarm-empty-chair-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();

        let error = add_agent(
            &connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap_err()
        .to_string();

        assert_eq!(error, "swarm: adapter gave no pane for orchestrator");
        assert!(
            swarm::store::agents(&connection, &session)
                .unwrap()
                .is_empty()
        );
    }
}
