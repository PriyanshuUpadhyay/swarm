//! Shared fixtures for CLI tests.

use super::*;
pub(super) use fixtures::*;

mod fixtures {
    use super::*;

    pub(crate) const ORCHESTRATOR: &str = "orchestrator";
    pub(crate) const CODER: &str = "coder";

    /// A session whose coder pane `%2` shows the file `<root>/screen`, which the fake adapter's
    /// `capture` reads; `verbs` adds the `ring` verb and any other, with `{screen}` for that path.
    pub(crate) fn ring_session(
        name: &str,
        provider: Option<&str>,
        verbs: &str,
        screen: &str,
    ) -> (std::path::PathBuf, rusqlite::Connection, String) {
        let root = std::env::temp_dir().join(format!("swarm-{name}-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        std::fs::write(root.join("screen"), screen).unwrap();
        let verbs = verbs.replace("{screen}", &root.join("screen").display().to_string());
        std::fs::write(
            root.join("adapters/fake.conf"),
            format!(
                "self = true\nspawn = true\nlist = echo %1 %2\nclose = true\n\
                 capture = cat '{}'\n{verbs}\n",
                root.join("screen").display()
            ),
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        if let Some(provider) = provider {
            swarm::store::set_provider(&connection, &session, CODER, provider).unwrap();
        }
        (root, connection, session)
    }

    pub(crate) fn delivery_of(
        connection: &rusqlite::Connection,
        session: &str,
        seq: i64,
    ) -> Option<String> {
        swarm::store::messages(connection, session, -1)
            .unwrap()
            .into_iter()
            .find(|message| message.seq == seq)
            .unwrap()
            .delivery
    }

    pub(crate) fn send_task(
        root: &std::path::Path,
        connection: &mut rusqlite::Connection,
        session: &str,
    ) -> i64 {
        deliver(
            connection,
            root,
            "fake",
            session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap()
    }

    /// One sweep pass by the chair that must succeed, and its lines.
    pub(crate) fn swept(
        connection: &mut rusqlite::Connection,
        root: &std::path::Path,
        adapter: &swarm::adapter::Adapter,
        session: &str,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        sweep_once(connection, root, adapter, session, ORCHESTRATOR, &mut lines).unwrap();
        lines
    }

    /// The kinds and senders of the messages to the chair.
    pub(crate) fn chair_mail(
        connection: &rusqlite::Connection,
        session: &str,
    ) -> Vec<(String, String)> {
        swarm::store::messages(connection, session, -1)
            .unwrap()
            .into_iter()
            .filter(|message| message.recipient == ORCHESTRATOR)
            .map(|message| (message.sender, message.kind))
            .collect()
    }

    pub(crate) fn session_command(home: &std::path::Path, args: &[&str]) -> std::process::Output {
        let exe = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("swarm{}", std::env::consts::EXE_SUFFIX));
        std::process::Command::new(exe)
            .env_clear()
            .env("HOME", home)
            .env("SWARM_HOME", home)
            .args(args)
            .output()
            .unwrap()
    }

    pub(crate) fn stop_fixture(
        tag: &str,
        log: Option<&str>,
    ) -> (std::path::PathBuf, rusqlite::Connection, String) {
        let home = std::env::temp_dir().join(format!("swarm-stop-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".swarm")).unwrap();
        let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        if let Some(text) = log {
            let path = home.join("chat.jsonl");
            std::fs::write(&path, text).unwrap();
            swarm::store::set_log(&connection, &session, CODER, &path).unwrap();
        }
        (home, connection, session)
    }

    pub(crate) fn run_stop_hook(
        home: &std::path::Path,
        session: &str,
        provider: &str,
        event: &str,
        payload: &str,
    ) {
        use std::io::Write;
        use std::process::{Command, Stdio};
        // Cargo builds this executable for the integration tests in the same test run.
        let exe = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("swarm{}", std::env::consts::EXE_SUFFIX));
        let mut child = Command::new(exe)
            .env_clear()
            .env("HOME", home)
            .env("SWARM_HOME", home)
            .env("SWARM_SESSION_ID", session)
            .env("SWARM_AGENT_ID", CODER)
            .args(["hook", provider, event])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"{}\n");
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
