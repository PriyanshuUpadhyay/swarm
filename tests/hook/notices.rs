// Tests for notices.
use super::*;

#[test]
fn notify_stores_text_and_notices_returns_it_once_without_an_adapter() {
    let home = scratch("notice-store");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let title = "swarm: a\" & (do shell script \"touch pwned\") & \"";
    let body = "body $HOME `id`";
    let sent = swarm(
        &home,
        &[("SWARM_ADAPTER", "missing")],
        &["notify", title, "--body", body],
        "",
    );
    assert!(sent.status.success(), "{sent:?}");
    let result = swarm(
        &home,
        &[("SWARM_ADAPTER", "missing")],
        &["notices", "--json"],
        "",
    );
    assert!(result.status.success(), "{result:?}");
    let rows: serde_json::Value = serde_json::from_str(&stdout(&result)).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["title"], title);
    assert_eq!(rows[0]["body"], body);
    assert!(rows[0]["session_id"].is_null());
    assert!(rows[0]["agent_id"].is_null());
    assert!(rows[0]["id"].is_i64());
    let second = swarm(&home, &[], &["notices", "--json"], "");
    assert_eq!(stdout(&second), "[]\n");
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    assert!(
        connection
            .query_row("SELECT posted_at FROM notice", [], |r| r
                .get::<_, Option<i64>>(0))
            .unwrap()
            .is_some()
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn notify_records_the_chair_identity_and_refuses_a_worker() {
    let home = scratch("notice-chair");
    let created = swarm(&home, &[], &["session", "new", "lane"], "");
    assert!(created.status.success());
    let session = stdout(&created).trim().to_string();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::add_agent(&connection, &session, "orchestrator", "orchestrator").unwrap();
    let sent = swarm(
        &home,
        &[
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &["notify", "Done"],
        "",
    );
    assert!(sent.status.success(), "{sent:?}");
    let rows: serde_json::Value =
        serde_json::from_str(&stdout(&swarm(&home, &[], &["notices", "--json"], ""))).unwrap();
    assert_eq!(rows[0]["session_id"], session);
    assert_eq!(rows[0]["agent_id"], "orchestrator");
    let refused = swarm(
        &home,
        &[("SWARM_AGENT_ID", "coder")],
        &["notify", "Blocked"],
        "",
    );
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr),
        "swarm: only the chair notifies the owner; send it to the orchestrator with swarm send\n"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn hud_install_and_uninstall_refuse_a_worker_and_restart_does_not() {
    let home = scratch("hud-worker");
    for verb in ["install", "uninstall"] {
        let refused = swarm(&home, &[("SWARM_AGENT_ID", "coder")], &["hud", verb], "");
        assert!(!refused.status.success());
        assert_eq!(
            String::from_utf8_lossy(&refused.stderr),
            "swarm: only the owner turns the usage HUD on or off\n"
        );
    }
    // A release script in a worker pane reloads the HUD; with no login item there is nothing to do.
    let restarted = swarm(
        &home,
        &[("SWARM_AGENT_ID", "coder")],
        &["hud", "restart"],
        "",
    );
    assert!(restarted.status.success(), "{restarted:?}");
    assert!(!home.join("Library/LaunchAgents").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn notify_starts_the_helper_and_does_not_wait() {
    let home = scratch("notice-open");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let helper = home.join("SwarmNotify.app");
    std::fs::create_dir_all(&helper).unwrap();
    let env = [("SWARM_NOTIFY_APP", helper.to_str().unwrap())];
    std::fs::write(
        home.join("bin/open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/opened\"\nwhile [ ! -e \"$HOME/release-open\" ]; do sleep 0.01; done\nprintf 'released\\n' >> \"$HOME/opened\"\n",
    )
    .unwrap();
    std::thread::scope(|scope| {
        let home = &home;
        let (sent, received) = std::sync::mpsc::channel();
        scope.spawn(move || {
            let result = swarm(home, &env, &["notify", "Done"], "");
            let _ = sent.send(result);
        });
        // `open` stays blocked until notify returns. The timeout only bounds a failing run.
        let result = received.recv_timeout(std::time::Duration::from_secs(60));
        let arguments = opened(home, 1);
        // Release the stub before any assertion, also when notify waited for it.
        std::fs::write(home.join("release-open"), "").unwrap();
        let released = opened(home, 2);
        let result = result.expect("notify waited for open");
        assert!(result.status.success(), "{result:?}");
        assert_eq!(arguments.unwrap(), helper_start(home, &helper));
        assert_eq!(
            released.unwrap(),
            format!("{}released\n", helper_start(home, &helper))
        );
    });
    std::fs::remove_dir_all(home).unwrap();
}

/// A helper that exists but cannot be started is a visible stderr line. The notice row stays and
/// notify still exits 0.
#[test]
fn notify_says_so_on_stderr_when_open_cannot_start() {
    let home = scratch("notice-open-fails");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let helper = home.join("SwarmNotify.app");
    std::fs::create_dir_all(&helper).unwrap();
    // No `open` on PATH, so spawn fails with ENOENT.
    std::fs::remove_file(home.join("bin/open")).unwrap();
    let bin = home.join("bin");
    let env = [
        ("SWARM_NOTIFY_APP", helper.to_str().unwrap()),
        ("PATH", bin.to_str().unwrap()),
    ];
    let notified = swarm(&home, &env, &["notify", "Done"], "");
    assert!(notified.status.success(), "{notified:?}");
    let stderr = String::from_utf8_lossy(&notified.stderr);
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("swarm: notice helper did not start: "))
        .collect();
    assert_eq!(lines.len(), 1, "{stderr}");
    let rows: serde_json::Value =
        serde_json::from_str(&stdout(&swarm(&home, &[], &["notices", "--json"], ""))).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1, "{rows}");
    std::fs::remove_dir_all(home).unwrap();
}

/// The hook never fails over a helper that did not start.
#[test]
fn a_change_to_waiting_still_prints_empty_json_when_open_cannot_start() {
    let home = scratch("notice-hook-open-fails");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    assert!(
        swarm(
            &home,
            &[("SWARM_SESSION_ID", &session)],
            &["agent", "add", "coder", "coder"],
            ""
        )
        .status
        .success()
    );
    let helper = home.join("SwarmNotify.app");
    std::fs::create_dir_all(&helper).unwrap();
    std::fs::remove_file(home.join("bin/open")).unwrap();
    let bin = home.join("bin");
    let as_coder = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "coder"),
        ("SWARM_NOTIFY_APP", helper.to_str().unwrap()),
        ("PATH", bin.to_str().unwrap()),
    ];
    let reported = swarm(
        &home,
        &as_coder,
        &["hook", "claude"],
        r#"{"hook_event_name":"PermissionRequest"}"#,
    );
    assert!(reported.status.success(), "{reported:?}");
    assert_eq!(stdout(&reported), "{}\n", "{reported:?}");
    assert_eq!(coder_state(&home, &session)["state"], "blocked");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn notify_starts_nothing_when_the_helper_is_missing() {
    let home = scratch("notice-open-missing");
    let missing = home.join("missing.app");
    std::fs::write(
        home.join("bin/open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/opened\"\n",
    )
    .unwrap();
    let env = [("SWARM_NOTIFY_APP", missing.to_str().unwrap())];
    assert!(swarm(&home, &env, &["notify", "Done"], "").status.success());
    // No `open -b` fallback: LaunchServices could start any dev build of the full app.
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert!(!home.join("opened").exists());
    std::fs::remove_dir_all(home).unwrap();
}

/// ADR 0077: each change to waiting is one notice row, and it starts the notice helper, never
/// the full app. Hooks still call no adapter.
#[test]
fn a_change_to_waiting_stores_one_notice_and_starts_the_helper_not_the_app() {
    let home = scratch("notice-waiting");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    assert!(
        swarm(
            &home,
            &[("SWARM_SESSION_ID", &session)],
            &["agent", "add", "coder", "coder"],
            ""
        )
        .status
        .success()
    );
    std::fs::write(
        home.join("bin/open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/opened\"\n",
    )
    .unwrap();
    let helper = home.join("SwarmNotify.app");
    std::fs::create_dir_all(&helper).unwrap();
    let as_coder = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "coder"),
        ("SWARM_NOTIFY_APP", helper.to_str().unwrap()),
    ];
    // Codex runs PermissionRequest before its auto-reviewer, so that hook is no wait (ADR 0021).
    for (cli, event) in [
        ("claude", "PermissionRequest"),
        ("claude", "PermissionRequest"),
        ("claude", "PreToolUse"),
        ("claude", "PermissionRequest"),
        ("claude", "PreToolUse"),
        ("codex", "PermissionRequest"),
    ] {
        let reported = swarm(
            &home,
            &as_coder,
            &["hook", cli],
            &format!(r#"{{"hook_event_name":"{event}"}}"#),
        );
        assert!(reported.status.success(), "{reported:?}");
    }
    assert_eq!(coder_state(&home, &session)["state"], "working");
    assert_eq!(
        opened(&home, 2).unwrap(),
        helper_start(&home, &helper).repeat(2)
    );
    let rows: serde_json::Value =
        serde_json::from_str(&stdout(&swarm(&home, &[], &["notices", "--json"], ""))).unwrap();
    let folder = home.file_name().unwrap().to_str().unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(row["session_id"], session.as_str());
        assert_eq!(row["agent_id"], "coder");
        assert_eq!(row["title"], folder);
        assert_eq!(row["body"], "Blocked on a permission.");
    }
    assert_eq!(rows.as_array().unwrap().len(), 2);
    std::fs::remove_dir_all(home).unwrap();
}

/// `notices --due` takes every row and prints only those the owner's notice preferences post.
#[test]
fn notices_due_applies_the_notice_preferences() {
    let home = scratch("notice-due");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let muted = home.join("muted");
    let open_project = home.join("open");
    for folder in [&muted, &open_project] {
        std::fs::create_dir_all(folder).unwrap();
    }
    let in_muted =
        swarm::store::create_session(&connection, "lane", &muted.join("wt"), None, None).unwrap();
    let in_open =
        swarm::store::create_session(&connection, "lane", &open_project, None, None).unwrap();
    for session in [&in_muted, &in_open] {
        swarm::store::add_agent(&connection, session, "coder", "coder").unwrap();
    }
    swarm::store::add_agent(&connection, &in_open, "ended", "coder").unwrap();
    swarm::store::set_state(
        &connection,
        &in_open,
        "coder",
        "blocked",
        "hook",
        None,
        None,
        1,
    )
    .unwrap();
    let choices = |notices: &str| {
        std::fs::write(
            home.join(".swarm/choices.json"),
            format!(r#"{{"prefs":{{"notices":{notices}}}}}"#),
        )
        .unwrap();
    };
    let add_kind = |session: Option<&str>, agent: Option<&str>, kind, title: &str| {
        swarm::store::add_notice(&connection, session, agent, kind, title, "").unwrap();
    };
    let add = |session: Option<&str>, agent: Option<&str>, title: &str| {
        add_kind(session, agent, None, title);
    };
    let due = || -> Vec<(String, bool)> {
        let out: serde_json::Value =
            serde_json::from_str(&stdout(&swarm(&home, &[], &["notices", "--due"], ""))).unwrap();
        out.as_array()
            .unwrap()
            .iter()
            .map(|row| (row["title"].as_str().unwrap().into(), row["sound"] == true))
            .collect()
    };

    choices(&format!(
        r#"{{"done":false,"sound":false,"mutedProjects":["{}"]}}"#,
        muted.display()
    ));
    add(Some(&in_muted), Some("coder"), "muted");
    add(Some(&in_open), Some("coder"), "waiting");
    use swarm::store::notice_kind::{ASKS, DONE, FAILED, TEST};
    add_kind(None, None, Some(DONE), "done");
    // A shell `swarm notify` and a wait whose agent no longer waits are muted too.
    add(None, None, "notify");
    add(Some(&in_open), Some("ended"), "ended");

    // The done switch mutes every other row: a question or a failure needs the owner, and a
    // test notice from Settings checks delivery.
    for kind in [ASKS, FAILED, TEST] {
        add_kind(None, None, Some(kind), kind);
    }
    assert_eq!(
        due(),
        [
            ("waiting", false),
            (ASKS, false),
            (FAILED, false),
            (TEST, false)
        ]
        .map(|(title, sound)| (title.to_string(), sound))
    );
    assert_eq!(due(), []);

    choices(r#"{"post":false}"#);
    add(Some(&in_open), Some("coder"), "blocked");
    assert_eq!(due(), []);

    std::fs::remove_file(home.join(".swarm/choices.json")).unwrap();
    add(Some(&in_muted), Some("coder"), "default");
    assert_eq!(due(), [("default".to_string(), true)]);
    std::fs::remove_dir_all(home).unwrap();
}
