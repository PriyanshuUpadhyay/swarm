// Tests for state context.
use super::*;

#[test]
fn a_claude_permission_request_shows_the_coder_waiting() {
    let home = scratch("waiting");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let in_session = [("SWARM_SESSION_ID", session.as_str())];
    let added = swarm(&home, &in_session, &["agent", "add", "coder", "coder"], "");
    assert!(added.status.success());

    let coder = coder_state(&home, &session);
    for field in ["state", "state_at_s", "state_source", "state_detail"] {
        assert!(coder[field].is_null(), "{field}: {coder}");
    }

    let permission = r#"{"hook_event_name":"PermissionRequest","session_id":"abc"}"#;
    let outside = swarm(&home, &[], &["hook", "claude"], permission);
    assert!(outside.status.success());
    assert_eq!(stdout(&outside), "{}\n");
    assert!(coder_state(&home, &session)["state"].is_null());

    let as_coder = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "coder"),
    ];
    let reported = swarm(&home, &as_coder, &["hook", "claude"], permission);
    assert!(reported.status.success());
    assert_eq!(stdout(&reported), "{}\n");
    let coder = coder_state(&home, &session);
    assert_eq!(coder["state"], "waiting");
    assert_eq!(coder["state_source"], "hook");
    assert!(coder["state_at_s"].as_i64().unwrap() > 0);
    // ADR 0077: the change to waiting is one notice row for the helper.
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM notice", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );

    let broken = swarm(&home, &as_coder, &["hook", "claude"], "not json");
    assert!(broken.status.success());
    assert_eq!(stdout(&broken), "{}\n");
    assert!(!broken.stderr.is_empty());
}

#[test]
fn the_chair_agent_log_becomes_the_chat_log() {
    let home = scratch("chair-log");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let in_session = [("SWARM_SESSION_ID", session.as_str())];
    // `session new` makes the orchestrator, the chair.
    assert!(
        swarm(&home, &in_session, &["agent", "add", "coder", "coder"], "")
            .status
            .success()
    );
    let chair_log = || -> Option<String> {
        swarm::store::open(&home.join(".swarm/swarm.db"))
            .unwrap()
            .query_row(
                "SELECT chair_log FROM session WHERE id = ?1",
                [&session],
                |r| r.get(0),
            )
            .unwrap()
    };
    // A child's log is its own; only the chair the app launched writes the chat's log.
    for agent in ["coder", "orchestrator"] {
        let log = home.join(format!("{agent}.jsonl"));
        std::fs::write(&log, "").unwrap();
        let payload =
            serde_json::json!({"hook_event_name": "SessionStart", "transcript_path": log});
        let env = [
            ("SWARM_SESSION_ID", session.as_str()),
            ("SWARM_AGENT_ID", agent),
        ];
        assert!(
            swarm(&home, &env, &["hook", "claude"], &payload.to_string())
                .status
                .success()
        );
        if agent == "coder" {
            assert_eq!(chair_log(), None);
        }
    }
    assert_eq!(
        chair_log().unwrap(),
        home.join("orchestrator.jsonl").to_string_lossy()
    );
}

#[test]
fn a_pi_chair_names_its_session_and_log() {
    let home = scratch("pi-chair");
    let sessions = home.join(".pi/agent/sessions/--work--");
    std::fs::create_dir_all(&sessions).unwrap();
    let log = |id: &str| {
        let path = sessions.join(format!("2026-10-14T10-00-00-000Z_{id}.jsonl"));
        std::fs::write(
            &path,
            format!("{{\"type\":\"session\",\"version\":3,\"id\":\"{id}\"}}\n"),
        )
        .unwrap();
        path
    };
    let row = |session: &str| -> (Option<String>, Option<String>, Option<String>) {
        swarm::store::open(&home.join(".swarm/swarm.db"))
            .unwrap()
            .query_row(
                "SELECT chair_provider, chair_id, chair_log FROM session WHERE id = ?1",
                [session],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    };
    // Pi gives each tool command its session id and file, so `session new` records both.
    let first = log("pi-one");
    let first_text = first.to_string_lossy().into_owned();
    let pi_env = [
        ("PI_SESSION_ID", "pi-one"),
        ("PI_SESSION_FILE", first_text.as_str()),
    ];
    let session = stdout(&swarm(&home, &pi_env, &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    assert_eq!(
        row(&session),
        (Some("pi".into()), Some("pi-one".into()), Some(first_text))
    );
    // A lane that started with no chair names it later; the listing finds its file by id.
    let second = log("pi-two");
    let caller = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    swarm::store::add_agent(
        &swarm::store::open(&home.join(".swarm/swarm.db")).unwrap(),
        &session,
        "orchestrator",
        "orchestrator",
    )
    .unwrap();
    let named = swarm(&home, &caller, &["session", "chair", "pi:pi-two"], "");
    assert!(
        named.status.success(),
        "{}",
        String::from_utf8_lossy(&named.stderr)
    );
    let second_text = second.to_string_lossy().into_owned();
    assert_eq!(row(&session).2.as_deref(), Some(second_text.as_str()));
    // The listing finds the file by id too, as it does for a row whose log was never recorded.
    swarm::store::open(&home.join(".swarm/swarm.db"))
        .unwrap()
        .execute(
            "UPDATE session SET chair_log = NULL WHERE id = ?1",
            [&session],
        )
        .unwrap();
    assert!(
        swarm(&home, &[], &["sessions", "--json"], "")
            .status
            .success()
    );
    assert_eq!(
        row(&session),
        (
            Some("pi".into()),
            Some("pi-two".into()),
            Some(second.to_string_lossy().into_owned())
        )
    );
}

#[test]
fn a_pane_status_feed_sets_the_agent_state() {
    let home = scratch("status-feed");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::add_agent(&connection, &session, "coder", "coder").unwrap();
    let env = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "coder"),
    ];
    let state = || -> (Option<String>, Option<String>) {
        connection
            .query_row(
                "SELECT state, state_kind FROM agent WHERE session_id = ?1 AND id = 'coder'",
                [&session],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    };
    // The pane's raw output: screen text with pi's reports in it. The start-up idle is no state.
    let fed = swarm(
        &home,
        &env,
        &["status-feed"],
        "\x1b[2J\x1b]7501;state=idle:app=pi\x1b\\hello\x1b]7501;state=working:app=pi\x07",
    );
    assert!(
        fed.status.success(),
        "{}",
        String::from_utf8_lossy(&fed.stderr)
    );
    assert_eq!(state(), (Some("working".into()), None));
    swarm(
        &home,
        &env,
        &["status-feed"],
        "\x1b]7501;state=blocked:kind=permission:app=pi\x07",
    );
    assert_eq!(state(), (Some("waiting".into()), Some("permission".into())));
    let notices: serde_json::Value =
        serde_json::from_str(&stdout(&swarm(&home, &[], &["notices", "--json"], ""))).unwrap();
    assert_eq!(notices[0]["agent_id"], "coder");
    swarm(
        &home,
        &env,
        &["status-feed"],
        "\x1b]7501;state=done:app=pi\x07",
    );
    assert_eq!(state(), (Some("done".into()), None));
}

#[test]
fn a_pane_status_feed_stores_the_pi_log_named_by_its_session_id() {
    let home = scratch("status-feed-pi-log");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::add_agent(&connection, &session, "orchestrator", "chair").unwrap();
    swarm::store::add_agent(&connection, &session, "coder", "coder").unwrap();
    // `launch` gave each pi agent the session id `<session>-<agent>` (ADR 0086).
    let pi = home.join("pi");
    let folder = pi.join("sessions/--tmp-proj--");
    std::fs::create_dir_all(&folder).unwrap();
    for agent in ["orchestrator", "coder"] {
        let log = folder.join(format!("2026-10-14T00-00-00-000Z_{session}-{agent}.jsonl"));
        std::fs::write(&log, "{}\n").unwrap();
        let fed = swarm(
            &home,
            &[
                ("SWARM_SESSION_ID", session.as_str()),
                ("SWARM_AGENT_ID", agent),
                ("PI_CODING_AGENT_DIR", pi.to_str().unwrap()),
            ],
            &["status-feed"],
            "\x1b]7501;state=working:app=pi\x07",
        );
        assert!(
            fed.status.success(),
            "{}",
            String::from_utf8_lossy(&fed.stderr)
        );
        let stored: Option<String> = connection
            .query_row(
                "SELECT log FROM agent WHERE session_id = ?1 AND id = ?2",
                [&session, agent],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored.as_deref(), log.to_str());
    }
    let chair: Option<String> = connection
        .query_row(
            "SELECT chair_log FROM session WHERE id = ?1",
            [&session],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        chair
            .unwrap()
            .ends_with(&format!("_{session}-orchestrator.jsonl"))
    );
}

#[test]
fn a_pi_log_written_after_the_working_report_is_stored_from_later_output() {
    use std::io::Write;
    let home = scratch("status-feed-pi-late-log");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::add_agent(&connection, &session, "orchestrator", "chair").unwrap();
    let pi = home.join("pi");
    let folder = pi.join("sessions/--tmp-proj--");
    std::fs::create_dir_all(&folder).unwrap();
    let mut command = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), &home);
    let mut feed = command
        .env("SWARM_SESSION_ID", &session)
        .env("SWARM_AGENT_ID", "orchestrator")
        .env("PI_CODING_AGENT_DIR", &pi)
        .current_dir(&home)
        .arg("status-feed")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = feed.stdin.take().unwrap();
    input
        .write_all(b"\x1b]7501;state=working:app=pi\x07")
        .unwrap();
    input.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    // pi makes its file after the report, and the turn then prints output with no report.
    let log = folder.join(format!(
        "2026-10-16T00-00-00-000Z_{session}-orchestrator.jsonl"
    ));
    std::fs::write(&log, "{}\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    input.write_all(b"thinking...").unwrap();
    input.flush().unwrap();
    let chair = || -> Option<String> {
        connection
            .query_row(
                "SELECT chair_log FROM session WHERE id = ?1",
                [&session],
                |r| r.get(0),
            )
            .unwrap()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while chair().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // The turn is still running: the feed has not closed.
    assert_eq!(chair().as_deref(), log.to_str());
    drop(input);
    assert!(feed.wait().unwrap().success());
}

#[test]
fn host_context_refuses_pi_which_gets_its_context_at_launch() {
    let home = scratch("host-context-pi");
    let refused = swarm(&home, &[], &["host-context", "--provider", "pi"], "");
    assert!(!refused.status.success());
    assert!(!stdout(&refused).contains("injectSteps"));
    assert!(
        swarm(&home, &[], &["host-context", "--provider", "agy"], "")
            .status
            .success()
    );
}

/// A seat's SessionStart context names its stored role, so the seat knows its job without
/// a pane variable for it.
#[test]
fn host_context_names_the_seat_and_its_stored_role() {
    let home = scratch("host-context-seat");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let session = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
    swarm::store::add_agent(&connection, &session, "rev-1", "review.check").unwrap();
    let seat = [
        ("SWARM_ADAPTER", "rex"),
        ("REX_BLOCK", "block:seat"),
        ("TERM_PROGRAM", "rex"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "rev-1"),
    ];
    for provider in ["claude", "codex", "agy"] {
        let output = swarm(
            &home,
            &seat,
            &["host-context", "--provider", provider],
            "{}",
        );
        assert!(output.status.success(), "{provider}: {output:?}");
        assert!(
            stdout(&output).contains(&format!(
                "You are seat `rev-1` with role `review.check` in swarm session `{session}`."
            )),
            "{provider}: {}",
            stdout(&output)
        );
    }
}

/// A seat's start hook in a home with no swarm database prints its context and makes no database.
#[test]
fn host_context_makes_no_database_for_a_seat_role() {
    let home = scratch("host-context-no-db");
    let seat = [
        ("SWARM_ADAPTER", "rex"),
        ("REX_BLOCK", "block:seat"),
        ("TERM_PROGRAM", "rex"),
        ("SWARM_SESSION_ID", "01a15391-109c-75b2-b09c-5eac0d553e25"),
        ("SWARM_AGENT_ID", "rev-1"),
    ];
    let output = swarm(
        &home,
        &seat,
        &["host-context", "--provider", "claude"],
        "{}",
    );
    assert!(output.status.success(), "{output:?}");
    assert!(
        stdout(&output).contains("You are seat `rev-1` in swarm session"),
        "{}",
        stdout(&output)
    );
    // Nor the marker that claims the home: a start hook only reads.
    assert!(!home.join(".swarm/swarm-home").exists());
    let _ = std::fs::remove_dir_all(&home);
}
