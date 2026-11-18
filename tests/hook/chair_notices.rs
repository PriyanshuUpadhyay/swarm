// Tests for the notices a chair's turn end stores (ADR 0138, ADR 0147).
use super::*;

/// ADR 0138: the chair's turn end is one done notice; a child's turn end is none, and so is a
/// chair turn end while a child still works.
#[test]
fn only_the_chair_turn_end_with_no_working_child_stores_a_done_notice() {
    let home = scratch("notice-chair-done");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::add_agent(&connection, &session, "orchestrator", "orchestrator").unwrap();
    swarm::store::add_agent(&connection, &session, "coder", "coder").unwrap();
    let missing = home.join("missing.app");
    let turn_on = |provider: &str, agent: &str, event: &str| {
        let env = [
            ("SWARM_SESSION_ID", session.as_str()),
            ("SWARM_AGENT_ID", agent),
            ("SWARM_NOTIFY_APP", missing.to_str().unwrap()),
        ];
        let payload = format!(r#"{{"hook_event_name":"{event}"}}"#);
        assert!(
            swarm(&home, &env, &["hook", provider], &payload)
                .status
                .success()
        );
    };
    let turn = |agent: &str, event: &str| turn_on("claude", agent, event);
    let notices = || -> Vec<serde_json::Value> {
        serde_json::from_str(&stdout(&swarm(&home, &[], &["notices", "--json"], ""))).unwrap()
    };
    turn("coder", "UserPromptSubmit");
    turn("orchestrator", "UserPromptSubmit");
    turn("orchestrator", "Stop");
    assert!(
        notices().is_empty(),
        "a working child holds the chair's notice"
    );
    turn("coder", "Stop");
    assert!(notices().is_empty(), "a child's turn end is no notice");
    turn("orchestrator", "UserPromptSubmit");
    // ADR 0147: a turn under 30 s posts nothing, so this turn began a minute ago.
    connection
        .execute("UPDATE agent SET turn_at = turn_at - 60", [])
        .unwrap();
    turn("orchestrator", "Stop");
    turn("orchestrator", "Stop");
    // A turn the owner stopped with Esc posts nothing, because the owner was there.
    turn_on("codex", "orchestrator", "UserPromptSubmit");
    connection
        .execute("UPDATE agent SET turn_at = turn_at - 60", [])
        .unwrap();
    turn_on("codex", "orchestrator", "Interrupt");
    let rows = notices();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["agent_id"], "orchestrator");
    assert_eq!(rows[0]["body"], "The orchestrator is done.");
    std::fs::remove_dir_all(home).unwrap();
}

/// ADR 0147: a chair turn end names the chat and quotes its reply, a reply that asks is an
/// `asks` row, a failed turn is a `failed` row, a short done turn posts nothing, and the inbox
/// keeps each chat's newest row until the owner looks.
#[test]
fn a_chair_turn_end_names_the_chat_quotes_the_reply_and_stays_in_the_inbox_until_seen() {
    let home = scratch("notice-inbox");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::add_agent(&connection, &session, "orchestrator", "orchestrator").unwrap();
    let log = home.join("chair.jsonl");
    std::fs::write(
        &log,
        "{\"type\":\"ai-title\",\"aiTitle\":\"Notice redesign\"}\n",
    )
    .unwrap();
    let missing = home.join("missing.app");

    let hook = |payload: serde_json::Value| {
        let env = [
            ("SWARM_SESSION_ID", session.as_str()),
            ("SWARM_AGENT_ID", "orchestrator"),
            ("SWARM_NOTIFY_APP", missing.to_str().unwrap()),
        ];
        let mut payload = payload;
        payload["transcript_path"] = log.to_str().unwrap().into();
        let out = swarm(&home, &env, &["hook", "claude"], &payload.to_string());
        assert!(out.status.success(), "{out:?}");
    };
    let long_turn = |end: serde_json::Value| {
        hook(serde_json::json!({"hook_event_name": "UserPromptSubmit"}));
        connection
            .execute("UPDATE agent SET turn_at = turn_at - 60", [])
            .unwrap();
        hook(end);
    };
    let inbox = || -> serde_json::Value {
        let out = swarm(&home, &[], &["notices", "--inbox", "--json"], "");
        assert!(out.status.success(), "{out:?}");
        serde_json::from_str(&stdout(&out)).unwrap()
    };

    // A list number and an abbreviation end no sentence.
    long_turn(serde_json::json!({
        "hook_event_name": "Stop",
        "last_assistant_message": "1. **Added chat titles to notices (e.g. HUD rows).** 212 tests pass.\n\nNot landed yet.",
    }));
    let rows = inbox()["rows"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 1, "{rows}");
    assert_eq!(rows[0]["session_id"], session.as_str());
    assert_eq!(rows[0]["title"], "Notice redesign");
    assert_eq!(
        rows[0]["body"],
        "Added chat titles to notices (e.g. HUD rows)."
    );
    assert_eq!(rows[0]["kind"], "done");
    let read = rows[0]["id"].to_string();

    // The chat keeps one row in the inbox, its newest: a turn that ends with a question. A
    // number ends a sentence.
    long_turn(serde_json::json!({
        "hook_event_name": "Stop",
        "last_assistant_message": "Merged PR 142. The review is clean.\n\nShould I land it?",
    }));
    let rows = inbox()["rows"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 1, "{rows}");
    assert_eq!(rows[0]["kind"], "asks");
    assert_eq!(rows[0]["body"], "Merged PR 142.");

    // A Clear of the HUD's older read keeps the question that came after it.
    let seen = swarm(
        &home,
        &[],
        &["notices", "seen", "--through", &read, &session],
        "",
    );
    assert!(seen.status.success(), "{seen:?}");
    assert_eq!(inbox()["rows"][0]["kind"], "asks");
    swarm(&home, &[], &["notices", "seen", &session], "");
    assert_eq!(inbox()["rows"], serde_json::json!([]));

    long_turn(serde_json::json!({
        "hook_event_name": "StopFailure",
        "error": "API Error: Rate limit reached",
    }));
    let rows = inbox()["rows"].clone();
    assert_eq!(rows[0]["kind"], "failed");
    assert_eq!(rows[0]["body"], "API Error: Rate limit reached");

    // A file name ends a sentence.
    long_turn(serde_json::json!({
        "hook_event_name": "Stop",
        "last_assistant_message": "Edited src/main.rs. Tests pass.",
    }));
    assert_eq!(inbox()["rows"][0]["body"], "Edited src/main.rs.");

    // A short done turn posts nothing.
    swarm(&home, &[], &["notices", "seen", &session], "");
    hook(serde_json::json!({"hook_event_name": "UserPromptSubmit"}));
    hook(serde_json::json!({"hook_event_name": "Stop", "last_assistant_message": "Done."}));
    assert_eq!(inbox()["rows"], serde_json::json!([]));
    std::fs::remove_dir_all(home).unwrap();
}
