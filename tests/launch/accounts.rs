// Account usage, fallback, and deadline tests.

use super::*;

#[test]
fn invalid_codex_home_keeps_real_auth_read_failure_fail_open() {
    let home = scratch("work-invalid-home-read-failure");
    trust_session(&home);
    std::fs::create_dir_all(home.join(".codex-Work")).unwrap();
    tool(&home, "codex", "exit 1");
    let output = swarm(&home, &[], &["roles", "get", "review.deep"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let resolved: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(resolved["runnerId"], "review.deep#1");
    assert_eq!(resolved["skipped"], serde_json::json!([]));
}

#[test]
fn named_launch_reuses_the_role_probe_deadline() {
    let home = scratch("work-account-deadline");
    std::fs::create_dir_all(home.join(".claude/.profiles/work")).unwrap();
    tool(
        &home,
        "claude",
        "echo probe >> \"$HOME/native-reads\"; exec /bin/sleep 25",
    );
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    ).unwrap();
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
    let start = std::time::Instant::now();
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "review.deep",
            "--provider",
            "claude",
            "--account",
            "work",
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("authentication is unavailable"),
        "{}",
        stderr(&output)
    );
    assert!(
        start.elapsed() < std::time::Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
    // One read per home (default and work), all started at once under the one deadline.
    assert_eq!(
        std::fs::read_to_string(home.join("native-reads"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

fn fable_launch_cache(home: &Path, work_fb: i64, default_fb: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let accounts: Vec<_> = [
        ("work", ".claude/.profiles/work", 10, work_fb),
        ("default", ".claude", 50, default_fb),
    ]
    .into_iter()
    .map(|(name, dir, general, fb)| {
        let meters: Vec<_> = [("5h", general), ("fb", fb)]
            .into_iter()
            .map(|(window, used)| {
                serde_json::json!({
                    "provider":"claude", "account":name, "label":name, "window":window,
                    "used_pct":used, "state":"fresh", "as_of_seconds":now,
                    "reset_time_seconds":now+3600
                })
            })
            .collect();
        serde_json::json!({"home":home.join(dir), "meters":meters})
    })
    .collect();
    std::fs::write(
        home.join(".swarm/claude-usage.json"),
        serde_json::json!({"accounts":accounts}).to_string(),
    )
    .unwrap();
}

#[test]
fn launch_uses_model_usage_and_refuses_spent_fable_auto_before_starting_a_pane() {
    let (home, session, cwd) = account_launch_fixture("fable-model-launch");
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let launch = |seat: &str, model: &str, account: Option<&str>| {
        let mut args = vec![
            "launch",
            seat,
            "chat",
            "--provider",
            "claude",
            "--model",
            model,
            "--cwd",
            &cwd,
        ];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        swarm(&home, &env, &args)
    };
    fable_launch_cache(&home, 100, 100);
    let refused = launch("spent", "best", None);
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("every signed-in Claude account has used its Fable limit"));
    assert!(
        !home
            .join(format!(".swarm/runs/{session}/spent.sh"))
            .exists()
    );
    let connection = rusqlite::Connection::open(home.join(".swarm/swarm.db")).unwrap();
    let started: i64 = connection
        .query_row("SELECT count(*) FROM agent WHERE id = 'spent'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(started, 0);
    let named = launch("named", "best", Some("work"));
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(stderr(&named).contains("account work"));
    let sonnet = launch("general", "sonnet", None);
    assert!(sonnet.status.success(), "{}", stderr(&sonnet));
    assert!(stderr(&sonnet).contains("account work"));
    fable_launch_cache(&home, 95, 10);
    let fable = launch("unspent", "best", None);
    assert!(fable.status.success(), "{}", stderr(&fable));
    assert!(stderr(&fable).contains("account default"));
}

fn renewable_launch_cache(home: &Path, work_used: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let work = serde_json::json!({
        "provider":"claude", "account":"work", "label":"work", "window":"5h",
        "used_pct":work_used, "state":"fresh", "as_of_seconds":now,
        "reset_time_seconds":now+3600
    });
    let default = serde_json::json!({
        "provider":"claude", "account":"default", "label":"default",
        "state":"failed", "reason":"Token expired", "reason_code":"token_expired"
    });
    std::fs::write(
        home.join(".swarm/claude-usage.json"),
        serde_json::json!({"accounts":[
            {"home":home.join(".claude/.profiles/work"), "meters":[work]},
            {"home":home.join(".claude"), "meters":[default]},
        ]})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn auto_takes_a_renewable_account_only_when_no_fresh_one_has_enough_left() {
    let (home, session, cwd) = account_launch_fixture("renewable-pick");
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let launch = |seat: &str| {
        swarm(
            &home,
            &env,
            &[
                "launch",
                seat,
                "chat",
                "--provider",
                "claude",
                "--model",
                "sonnet",
                "--cwd",
                &cwd,
            ],
        )
    };
    renewable_launch_cache(&home, 60);
    let fresh = launch("fresh");
    assert!(fresh.status.success(), "{}", stderr(&fresh));
    assert!(
        stderr(&fresh).contains("account work"),
        "{}",
        stderr(&fresh)
    );
    renewable_launch_cache(&home, 97);
    let renewable = launch("renewable");
    assert!(renewable.status.success(), "{}", stderr(&renewable));
    assert!(
        stderr(&renewable).contains("account default"),
        "{}",
        stderr(&renewable)
    );
    let role = swarm(&home, &[], &["roles", "get", "chat"]);
    assert!(role.status.success(), "{}", stderr(&role));
    let role: serde_json::Value = serde_json::from_slice(&role.stdout).unwrap();
    assert_eq!(role["provider"], "claude");
    assert_eq!(role["skipped"], serde_json::json!([]));
}

#[test]
fn an_auto_launch_warns_when_a_trust_file_cannot_be_edited_and_a_named_one_stops() {
    let (home, session, cwd) = account_launch_fixture("trust-file-auto");
    std::fs::write(home.join(".claude.json"), "[]\n").unwrap();
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let launch = |seat: &str, account: Option<&str>| {
        let mut args = vec![
            "launch",
            seat,
            "chat",
            "--provider",
            "claude",
            "--model",
            "sonnet",
            "--cwd",
            &cwd,
        ];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        swarm(&home, &env, &args)
    };
    for (seat, account) in [("plain", None), ("auto", Some("auto"))] {
        let output = launch(seat, account);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(
            stderr(&output).contains("swarm: skipped"),
            "{}",
            stderr(&output)
        );
    }
    let named = launch("named", Some("work"));
    assert!(!named.status.success());
    assert!(
        stderr(&named).contains("not a JSON object"),
        "{}",
        stderr(&named)
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".claude.json")).unwrap(),
        "[]\n"
    );
}

#[test]
fn roles_check_uses_fable_models_and_reads_native_accounts_once_per_provider() {
    let (home, session, cwd) = account_launch_fixture("fable-role-fallback");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes":{"review.scope":["claude-fable","claude-sonnet"]},"runners":{
            "claude-fable":{"provider":"claude","model":"fable","effort":"high"},
            "claude-sonnet":{"provider":"claude","model":"sonnet","effort":"high"}}}"#,
    )
    .unwrap();
    fable_launch_cache(&home, 100, 100);
    let output = swarm(&home, &[], &["roles", "check", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let check: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let scope = check["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["name"] == "review.scope")
        .unwrap();
    assert_eq!(scope["pick"], 1);
    assert_eq!(scope["skipped"][0]["code"], "fable_spent");
    assert_eq!(
        scope["skipped"][0]["text"],
        "every signed-in Claude account has used its Fable limit"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("native-reads"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(
        &home,
        &env,
        &["launch", "fallback", "review.scope", "--cwd", &cwd],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("running claude/sonnet/high"));
    assert!(stderr(&output).contains("account work"));
    let named = swarm(
        &home,
        &env,
        &[
            "launch",
            "named",
            "review.scope",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(stderr(&named).contains("running claude/fable/high"));
    fable_launch_cache(&home, 99, 99);
    let output = swarm(&home, &[], &["roles", "get", "review.scope"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let role: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(role["model"], "sonnet");
    assert!(
        role["skipped"][0]["text"]
            .as_str()
            .unwrap()
            .contains("usage 1% left")
    );
}

#[test]
fn slow_working_auth_is_read_once_by_role_and_named_launch() {
    let (home, session, cwd) = account_launch_fixture("work-slow-probe");
    std::fs::create_dir_all(home.join(".claude/.profiles/personal")).unwrap();
    tool(
        &home,
        "claude",
        r#"echo "$CLAUDE_CONFIG_DIR" >> "$HOME/native-reads"; /bin/sleep 0.1; echo '{"loggedIn":true}'"#,
    );
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "review.deep",
            "--provider",
            "claude",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("account work"));
    assert_eq!(
        std::fs::read_to_string(home.join("native-reads"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn account_budget_starts_after_the_herdr_check() {
    let (home, session, cwd) = account_launch_fixture("personal-herdr-budget");
    let adapter = std::fs::read(home.join(".swarm/adapters/fake.conf")).unwrap();
    std::fs::write(home.join(".swarm/adapters/herdr.conf"), adapter).unwrap();
    tool(
        &home,
        "herdr",
        "test \"$*\" = status || exit 8; /bin/sleep 2.2",
    );
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "herdr"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "chat",
            "--model",
            "opus",
            "--provider",
            "claude",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("account work"));
}

#[test]
fn account_budget_starts_after_a_waiting_store_open() {
    let (home, session, cwd) = account_launch_fixture("spare-store-budget");
    let connection = rusqlite::Connection::open(home.join(".swarm/swarm.db")).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE;")
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2200));
        connection.execute_batch("COMMIT;").unwrap();
    });
    let started = std::time::Instant::now();
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "chat",
            "--model",
            "opus",
            "--provider",
            "claude",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    release.join().unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_secs(2));
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("account work"));
}
