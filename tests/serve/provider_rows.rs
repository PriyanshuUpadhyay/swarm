use super::*;

/// pi has no hook to report its log, so the server finds it by the session id that `launch` gave
/// pi, `<session>-<agent>` (ADR 0086).
#[test]
fn a_pi_log_is_found_by_its_session_id() {
    let server = Server::new();
    let sessions = server.dir.join("home/.pi/agent/sessions/--work--");
    fs::create_dir_all(&sessions).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/provider-logs/pi-chat.jsonl");
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    let chair = format!("{}-orchestrator", server.session);
    swarm::store::set_chair(&connection, &server.session, Some(("pi", &chair))).unwrap();
    swarm::store::add_agent(&connection, &server.session, "piseat", "chat").unwrap();
    swarm::store::set_provider(&connection, &server.session, "piseat", "pi").unwrap();
    for agent in ["chair", "piseat"] {
        let path = format!(
            "/api/rows?session={}&agent={agent}&tail=200",
            server.session
        );
        let (status, _, body) = server.request("GET", &path, "", "", true);
        assert_eq!(status, 404, "{agent} before its log: {body}");
    }
    for id in [chair, format!("{}-piseat", server.session)] {
        fs::copy(
            &fixture,
            sessions.join(format!("2026-10-14T10-00-00-000Z_{id}.jsonl")),
        )
        .unwrap();
    }
    for agent in ["chair", "piseat"] {
        let path = format!(
            "/api/rows?session={}&agent={agent}&tail=200",
            server.session
        );
        let (status, _, body) = server.request("GET", &path, "", "", true);
        assert_eq!(status, 200, "{agent}: {body}");
        for text in [
            "Run the tests",
            "One test fails.",
            "cargo test",
            "overloaded",
        ] {
            assert!(body.contains(text), "{agent}: {text} missing from {body}");
        }
    }
}

/// A new Claude chair's hook stores its log only at the end of its first turn, but the file exists
/// from the turn's start. The server finds it by the chair's id, so the page can show that turn
/// live (ADR 0131).
#[test]
fn a_claude_chair_log_is_found_by_its_id_before_its_hook_stores_it() {
    let server = Server::new();
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    let id = uuid::Uuid::now_v7().to_string();
    swarm::store::set_chair(&connection, &server.session, Some(("claude", &id))).unwrap();
    let chair_log = || {
        let (status, _, body) = server.request("GET", "/api/snapshot", "", "", true);
        assert_eq!(status, 200, "{body}");
        let snapshot: Value = serde_json::from_str(&body).unwrap();
        snapshot["sessions"][0]["chair_log"].clone()
    };
    let rows = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    assert_eq!(chair_log(), Value::Null);
    assert_eq!(server.request("GET", &rows, "", "", true).0, 404);
    let project = server.dir.join("home/.claude/projects/-work-");
    fs::create_dir_all(&project).unwrap();
    let log = project.join(format!("{id}.jsonl"));
    fs::copy(&server.log, &log).unwrap();
    assert_eq!(chair_log(), json!(log.to_string_lossy()));
    let (status, _, body) = server.request("GET", &rows, "", "", true);
    assert_eq!(status, 200, "{body}");
    let page: Value = serde_json::from_str(&body).unwrap();
    assert!(page["rows"].as_array().unwrap().iter().any(|row| {
        row["kind"] == "assistant" && row["text"] == "I will look at the build first."
    }));
}

#[test]
fn rows_return_assistant_text_for_codex_and_agy_logs() {
    let server = Server::new();
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    for (provider, log, text) in [
        (
            "codex",
            include_str!("../fixtures/provider-logs/codex-chair-run.jsonl"),
            "Waiting for the last reply.",
        ),
        (
            "agy",
            include_str!("../fixtures/provider-logs/agy-chair-run.jsonl"),
            "The reviewer approved the change.",
        ),
    ] {
        let path = server.dir.join(format!("{provider}.jsonl"));
        fs::write(&path, log).unwrap();
        swarm::store::add_agent(&connection, &server.session, provider, "chat").unwrap();
        swarm::store::set_provider(&connection, &server.session, provider, provider).unwrap();
        swarm::store::set_log(&connection, &server.session, provider, &path).unwrap();
        let route = format!(
            "/api/rows?session={}&agent={provider}&tail=200",
            server.session
        );
        let (status, _, body) = server.request("GET", &route, "", "", true);
        assert_eq!(status, 200, "{provider}: {body}");
        let page: Value = serde_json::from_str(&body).unwrap();
        assert!(
            page["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| { row["kind"] == "assistant" && row["text"] == text }),
            "{provider}: {body}"
        );
    }
}
