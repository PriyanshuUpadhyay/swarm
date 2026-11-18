use super::*;

fn status(authorization: &str, style: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"authorization": authorization, "style": style}))
}

#[test]
fn judge_names_the_fix_for_each_state() {
    let cases = [
        (
            status("notDetermined", "banner"),
            Ok(None),
            "allow",
            Some("allow"),
        ),
        (
            status("denied", "banner"),
            Ok(Some(true)),
            "blocked",
            Some("notifications"),
        ),
        (
            status("authorized", "none"),
            Ok(None),
            "blocked",
            Some("notifications"),
        ),
        (
            status("authorized", "banner"),
            Ok(Some(true)),
            "held",
            Some("focus"),
        ),
        (status("authorized", "alert"), Ok(Some(false)), "ok", None),
        (status("provisional", "banner"), Ok(None), "ok", None),
        // A failed read, or a value this code does not know, never reads as ok.
        (Err("no answer".into()), Ok(Some(false)), "unknown", None),
        (status("unknown", "banner"), Ok(None), "unknown", None),
        (Ok(serde_json::json!({})), Ok(None), "unknown", None),
        (
            status("authorized", "banner"),
            Err("log failed".into()),
            "unknown",
            None,
        ),
    ];
    for (status, held, state, action) in cases {
        let health = judge(status, || held);
        assert_eq!(health["state"], state, "{health}");
        assert_eq!(health["action"].as_str(), action, "{health}");
    }
}

#[test]
fn held_in_reads_only_the_newest_presented_notice() {
    let presented = |ident: &str| {
        format!(
            "usernoted Presenting <NotificationRecord app:\"{BUNDLE}\" ident:\"{ident}\" req:\"X\">"
        )
    };
    let muted = |ident: &str| {
        format!("NotificationCenter {ident} ({BUNDLE}) muted by DND suppression: delay")
    };
    assert_eq!(held_in(""), Ok(None));
    assert_eq!(
        held_in(&[presented("old"), muted("old")].join("\n")),
        Ok(Some(true))
    );
    // NotificationCenter mutes the old notice again; the newest notice was shown.
    let log = [
        presented("old"),
        muted("old"),
        presented("newest"),
        muted("old"),
    ]
    .join("\n");
    assert_eq!(held_in(&log), Ok(Some(false)));
    // A record whose ident this cannot read is an error, so the health is unknown, not ok.
    assert!(held_in("usernoted Presenting <NotificationRecord app:\"x\">").is_err());
}

#[test]
fn the_settings_link_names_the_helper() {
    assert!(NOTIFICATIONS_URL.ends_with(BUNDLE));
}

/// ADR 0147: a wait lasts while its pane is open. A chair whose pane closed during a wait is no
/// "needs you" card, and its pane is forgotten so the working list drops it too.
#[test]
fn the_inbox_drops_a_wait_whose_pane_is_gone() {
    let root = std::env::temp_dir().join(format!("swarm-inbox-pane-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("adapters")).unwrap();
    // Only %1 is open; `screen` prints nothing, so the live card has no prompt.
    std::fs::write(
        root.join("adapters/fake.conf"),
        "self = true\nspawn = true\nring = true\nlist = echo %1\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
    let mut sessions = Vec::new();
    for pane in ["%1", "%2"] {
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            Some(("claude", "chair-id")),
            Some("fake"),
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, "orchestrator", "orchestrator").unwrap();
        swarm::store::set_pane(&connection, &session, "orchestrator", pane).unwrap();
        swarm::store::set_state(
            &connection,
            &session,
            "orchestrator",
            "blocked",
            "hook",
            None,
            None,
            1,
        )
        .unwrap();
        sessions.push(session);
    }

    let inbox = inbox(&root, &connection).unwrap();

    assert_eq!(inbox["needs_total"], 1, "{inbox}");
    assert_eq!(inbox["needs"][0]["session"], sessions[0].as_str());
    let running: Vec<_> = swarm::store::running_agents(&connection)
        .unwrap()
        .into_iter()
        .map(|agent| agent.session)
        .collect();
    assert_eq!(running, [sessions[0].clone()]);
    std::fs::remove_dir_all(root).unwrap();
}
