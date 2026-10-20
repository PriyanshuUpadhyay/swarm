// Agent listing and failed sweep tests.

use super::*;

#[test]
fn all_agent_listings_match_each_sessions_adapter_and_fields() {
    let home = scratch("all-agents");
    let adapters = home.join(".swarm/adapters");
    std::fs::create_dir_all(&adapters).unwrap();
    for (name, list) in [
        ("live", "printf live-pane"),
        ("dead", "true"),
        ("failed", "exit 1"),
    ] {
        std::fs::write(adapters.join(format!("{name}.conf")), format!(
            "self = true\nspawn = true\nring = true\nlist = test -d \"$HOME/sessions/$SWARM_SESSION_ID\" && test \"$SWARM_ADAPTER\" = {name} && test \"$SWARM_AGENT_ID\" = orchestrator || exit 1; {list}\nclose = true\ncapture = true\nscreen = cat \"$HOME/screen\"\nattach = true\n"
        )).unwrap();
    }
    std::fs::write(home.join("screen"), PERMISSION).unwrap();
    let log = home.join("agent.jsonl");
    std::fs::write(&log, "").unwrap();
    let mut connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut expected = serde_json::Map::new();
    for name in ["live", "dead", "failed"] {
        let session =
            swarm::store::create_session(&connection, "lane", &home, None, Some(name)).unwrap();
        std::fs::create_dir_all(home.join("sessions").join(&session)).unwrap();
        for (id, pane) in [
            ("coder", format!("{name}-pane")),
            ("unspawned", String::new()),
        ] {
            swarm::store::add_agent(&connection, &session, id, "code").unwrap();
            if !pane.is_empty() {
                swarm::store::set_pane(&connection, &session, id, &pane).unwrap();
            }
            swarm::store::set_provider(&connection, &session, id, "claude").unwrap();
            swarm::store::set_log(&connection, &session, id, &log).unwrap();
            swarm::store::set_state(
                &connection,
                &session,
                id,
                "failed",
                "hook",
                Some("fixture failure"),
                None,
                now,
            )
            .unwrap();
        }
        let output = swarm(
            &home,
            &[
                ("SWARM_ADAPTER", name),
                ("SWARM_SESSION_ID", &session),
                ("SWARM_AGENT_ID", "orchestrator"),
            ],
            &["agents", "--json"],
        );
        assert!(output.status.success(), "{}", stderr(&output));
        let listing: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            listing["agents"][0]["alive"],
            match name {
                "live" => serde_json::json!(true),
                "dead" => serde_json::json!(false),
                _ => serde_json::Value::Null,
            }
        );
        if name == "live" {
            assert!(listing["agents"][0]["prompt"].is_object());
        }
        expected.insert(session, listing);
    }
    // Unknown adapters and old sessions without an adapter must not borrow the process adapter.
    for adapter in [None, Some(" "), Some("missing"), Some("dead")] {
        let session =
            swarm::store::create_session(&connection, "lane", &home, None, adapter).unwrap();
        swarm::store::add_agent(&connection, &session, "coder", "code").unwrap();
        if adapter == Some("dead") {
            swarm::store::archive_sessions(&mut connection, &[session]).unwrap();
        }
    }
    swarm::store::create_session(&connection, "lane", &home, None, Some("live")).unwrap();
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "missing"),
            ("SWARM_SESSION_ID", "wrong-session"),
            ("SWARM_AGENT_ID", "wrong-agent"),
        ],
        &["agents", "--json", "--all"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, serde_json::Value::Object(expected));
}

/// A sweep pass that fails after it reported a dead child still prints `dead <id>`. The report
/// forgot the child's pane, so no later pass finds it dead again.
#[test]
fn a_failed_sweep_pass_still_prints_the_dead_child_it_reported() {
    let home = scratch("sweep-dead-line");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    // The first listing shows only the chair and `second`; the next one fails.
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\n\
         list = [ -e \"$HOME/listed\" ] && exit 1; touch \"$HOME/listed\"; printf 'chair p2\\n'\n\
         close = true\ncapture = true\n",
    )
    .unwrap();
    let session = swarm(
        &home,
        &[("SWARM_ADAPTER", "fake")],
        &["session", "new", "lane"],
    );
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    for args in [
        &["agent", "add", "orchestrator", "orchestrator"][..],
        &["agent", "add", "first", "coder"],
        &["agent", "add", "second", "coder"],
    ] {
        let output = swarm(&home, &chair(&session), args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_pane(&connection, &session, "first", "p1").unwrap();
    swarm::store::set_pane(&connection, &session, "second", "p2").unwrap();

    let sweep = swarm(&home, &chair(&session), &["sweep"]);
    assert!(!sweep.status.success());
    assert_eq!(String::from_utf8_lossy(&sweep.stdout), "dead first\n");
}
