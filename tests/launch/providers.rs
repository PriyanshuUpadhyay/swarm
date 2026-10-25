// Provider launch profiles and host contract tests.

use super::*;

/// New Chat with no pick runs the chat profile with its effort; a one-off pick runs exactly that
/// model with the chat profile's effort for that provider (ADR 0033).
#[test]
fn a_chat_launches_from_the_chat_profile_and_a_one_off_pick_keeps_its_effort() {
    let home = scratch("chat");
    tool(&home, "claude", "true");
    tool(&home, "codex", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(&home, &fake, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let script = |seat: &str| {
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/{seat}.sh"))).unwrap()
    };

    let profile = swarm(
        &home,
        &env,
        &["launch", "chat-profile", "chat", "--cwd", &cwd],
    );
    assert!(profile.status.success(), "{}", stderr(&profile));
    assert!(stderr(&profile).contains("swarm: chat: running claude/opus/high"));
    let command = script("chat-profile");
    assert!(
        command.contains("'--model' 'opus' '--effort' 'high'"),
        "{command}"
    );

    let one_off = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-one-off",
            "chat",
            "--provider",
            "codex",
            "--model",
            "gpt-6-luna",
            "--cwd",
            &cwd,
        ],
    );
    assert!(one_off.status.success(), "{}", stderr(&one_off));
    let command = script("chat-one-off");
    assert!(command.contains("'--model' 'gpt-6-luna'"), "{command}");
    assert!(
        command.contains(r#"'model_reasoning_effort="high"'"#),
        "{command}"
    );
    assert!(
        command.contains(r#"'default_permissions="swarm-code"'"#),
        "{command}"
    );

    // The app always sends `--account auto` for the chat profile; a provider with no accounts
    // launches on its own login instead of failing.
    tool(&home, "agy", "true");
    let agy = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-agy",
            "chat",
            "--provider",
            "agy",
            "--model",
            "flash",
            "--account",
            "auto",
            "--cwd",
            &cwd,
        ],
    );
    assert!(agy.status.success(), "{}", stderr(&agy));
    assert!(script("chat-agy").contains("'agy' '--model' 'flash'"));

    // A named account that no runner's provider has skips each runner before anything runs.
    let named = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-named",
            "chat",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(!named.status.success());
    let text = stderr(&named);
    assert!(
        text.contains("1 claude/opus/high: no claude account named work"),
        "{text}"
    );
    assert!(text.contains("no runner can run"), "{text}");
    assert!(!text.contains("running"), "{text}");

    // With unavailable native auth, `auto` falls back to the CLI's own login.
    let no_usage = swarm(
        &home,
        &env,
        &["launch", "chat-no-usage", "chat", "--cwd", &cwd],
    );
    assert!(no_usage.status.success(), "{}", stderr(&no_usage));
    assert!(stderr(&no_usage).contains("claude uses its own login"));
    assert!(
        script("chat-no-usage").contains(
            "'env' '-u' 'CLAUDE_CONFIG_DIR' '-u' 'CLAUDE_SECURESTORAGE_CONFIG_DIR' '-u' 'AGENT_PROFILE_LABEL' '--' 'claude'"
        )
    );
    assert!(script("chat-no-usage").contains("'--model' 'opus' '--effort' 'high'"));

    // An unavailable native read keeps the scheduler fail-open and Auto on the CLI login.
    tool(
        &home,
        "claude",
        "if [ \"$*\" = 'auth status' ]; then exec /bin/sleep 25; fi",
    );
    let started = std::time::Instant::now();
    let stuck = swarm(
        &home,
        &[&env[..], &[("SWARM_TEST_ACCOUNT_PICK_MS", "2000")]].concat(),
        &[
            "launch",
            "chat-stuck-auth",
            "chat",
            "--account",
            "auto",
            "--cwd",
            &cwd,
        ],
    );
    assert!(stuck.status.success(), "{}", stderr(&stuck));
    // A hang guard: the launch ends at the 2 s pick deadline, before the 25 s hang ends.
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert!(
        stderr(&stuck).contains("no automatic account for claude; claude uses its own login"),
        "{}",
        stderr(&stuck)
    );
}

#[test]
fn a_superset_chair_registers_as_orchestrator_on_session_new() {
    let home = scratch("superset-chair");
    let superset = [("SUPERSET_TERMINAL_ID", "t1")];
    let session = swarm(&home, &superset, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let inbox = swarm(&home, &superset, &["inbox"]);
    assert!(inbox.status.success(), "{}", stderr(&inbox));
}

#[test]
fn a_superset_terminal_counts_as_an_agent_pane_for_the_owner_only_guard() {
    let home = scratch("superset-guard");
    let output = swarm(
        &home,
        &[("SUPERSET_TERMINAL_ID", "t1")],
        &["answer", "w1", "p1", "1"],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("only the owner answers"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_rex_chair_registers_as_orchestrator_on_session_new() {
    let home = scratch("rex-chair");
    let rex = [("REX_BLOCK", "block:1"), ("TERM_PROGRAM", "rex")];
    let session = swarm(&home, &rex, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let inbox = swarm(&home, &rex, &["inbox"]);
    assert!(inbox.status.success(), "{}", stderr(&inbox));
}

#[test]
fn a_rex_block_counts_as_an_agent_pane_for_the_owner_only_guard() {
    let home = scratch("rex-guard");
    let output = swarm(
        &home,
        &[("REX_BLOCK", "block:1")],
        &["answer", "w1", "p1", "1"],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("only the owner answers"),
        "{}",
        stderr(&output)
    );
}

/// pi has no hooks: its chair is known at launch from the session id swarm gives it, it reads the
/// host contract from its system prompt, and it reports its state by OSC 7501 (ADR 0086).
#[test]
fn a_pi_chair_is_named_at_launch_and_gets_the_host_contract() {
    let home = scratch("pi-chair");
    tool(&home, "pi", "true");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"pi.chair": ["pi-haiku"]},
            "runners": {"pi-haiku": {"provider": "pi", "model": "anthropic/claude-haiku-5-5", "effort": "low"}}}"#,
    )
    .unwrap();
    // The app's adapter, with verbs that touch no tmux server.
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &["launch", "orchestrator", "pi.chair", "--cwd", &cwd],
    );
    assert!(output.status.success(), "{}", stderr(&output));

    let script =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
            .unwrap();
    let id = format!("{session}-orchestrator");
    assert!(
        script.contains(&format!(
            "'pi' '--model' 'anthropic/claude-haiku-5-5' '--thinking' 'low' '--session-id' '{id}' '--append-system-prompt' '[agent-host: swarm-app]"
        )),
        "{script}"
    );
    assert!(!script.contains("[agent-runtime"), "{script}");

    let sessions = swarm(&home, &[], &["sessions", "--json"]);
    let sessions: serde_json::Value = serde_json::from_slice(&sessions.stdout).unwrap();
    let row = &sessions["sessions"][0];
    assert_eq!(row["chair_provider"], "pi");
    assert_eq!(row["chair_id"], id.as_str());
}

/// Codex has no per-launch chair hook, so a Codex chair is named at launch, and its first log
/// gives its thread id. A chat with no chair provider gets no sidebar row and no transcript.
#[test]
fn a_codex_chair_is_named_at_launch_and_its_log_gives_the_thread_id() {
    let home = scratch("codex-chair");
    tool(&home, "codex", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &[
            "launch",
            "orchestrator",
            "chat",
            "--provider",
            "codex",
            "--model",
            "gpt-6-luna",
            "--cwd",
            &cwd,
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let chair = || {
        let sessions = swarm(&home, &[], &["sessions", "--json"]);
        let sessions: serde_json::Value = serde_json::from_slice(&sessions.stdout).unwrap();
        sessions["sessions"][0].clone()
    };
    assert_eq!(chair()["chair_provider"], "codex");
    assert!(chair()["chair_id"].is_null());

    let thread = "01a14385-ff51-7000-8000-000000000001";
    let log = home.join(format!("rollout-2026-10-16T12-33-37-{thread}.jsonl"));
    std::fs::write(&log, "").unwrap();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_chair_log(&connection, &session, &log).unwrap();
    assert_eq!(chair()["chair_provider"], "codex");
    assert_eq!(chair()["chair_id"], thread);
}

/// The owner's send starts a turn, so the agent lists as working at once, not when its own first
/// report comes a second or two later.
#[test]
fn a_typed_prompt_marks_the_agent_working_at_once() {
    let home = scratch("type-working");
    tool(&home, "codex", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        // An idle Codex input box, so the send does not wait for one to draw.
        "self = printf chair\nspawn = printf pane\nring = true\nlist = printf pane\nclose = true\ncapture = printf '› Ask'\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let launched = swarm(
        &home,
        &env,
        &[
            "launch",
            "orchestrator",
            "chat",
            "--provider",
            "codex",
            "--model",
            "m",
            "--cwd",
            &cwd,
        ],
    );
    assert!(launched.status.success(), "{}", stderr(&launched));
    let mut typed = Command::new(env!("CARGO_BIN_EXE_swarm"));
    typed
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .envs(app)
        .env("SWARM_SESSION_ID", &session)
        .current_dir(&home)
        .args(["type", "orchestrator"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = typed.spawn().unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"hello").unwrap();
    let typed = child.wait_with_output().unwrap();
    assert!(typed.status.success(), "{}", stderr(&typed));
    let listed = swarm(&home, &env, &["agents", "--json"]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let chair = listed["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "orchestrator")
        .unwrap();
    assert_eq!(chair["state"], "working", "{listed}");
    // Nothing is written, so the agent's own report decides the state.
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let state: Option<String> = connection
        .query_row(
            "SELECT state FROM agent WHERE session_id = ?1 AND id = 'orchestrator'",
            [&session],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, None);
}

/// A pi seat has no SessionStart hook, so its system prompt carries the same seat block, with
/// the role it was launched for.
#[test]
fn a_pi_seat_gets_its_seat_block_in_the_system_prompt() {
    let home = scratch("pi-seat");
    tool(&home, "pi", "true");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"pi.seat": ["pi-haiku"]},
            "runners": {"pi-haiku": {"provider": "pi", "model": "anthropic/claude-haiku-5-5", "effort": "low"}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.to_string_lossy().into_owned();
    let output = swarm(&home, &env, &["launch", "pi-1", "pi.seat", "--cwd", &cwd]);
    assert!(output.status.success(), "{}", stderr(&output));
    let script =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/pi-1.sh"))).unwrap();
    assert!(
        script.contains(&format!(
            "You are seat `pi-1` with role `pi.seat` in swarm session `{session}`."
        )),
        "{script}"
    );
}

/// tmux answers no OSC 7501 probe, so on an adapter whose pane feeds `swarm status-feed`, a CLI
/// that sends reports runs under `swarm pty`, which answers it; Codex sends none (ADR 0122).
#[test]
fn a_feed_pane_runs_claude_agy_and_pi_under_swarm_pty_and_codex_bare() {
    let home = scratch("pty-wrap");
    for cli in ["claude", "codex", "agy", "pi"] {
        tool(&home, cli, "true");
    }
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        "self = printf chair\nspawn = : \"$SWARM_FEED\"; printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    for (seat, provider, model) in [
        ("seat-claude", "claude", "opus"),
        ("seat-codex", "codex", "gpt-6-luna"),
        ("seat-agy", "agy", "flash"),
        ("seat-pi", "pi", "anthropic/claude-haiku-5-5"),
    ] {
        let output = swarm(
            &home,
            &env,
            &[
                "launch",
                seat,
                "chat",
                "--provider",
                provider,
                "--model",
                model,
                "--cwd",
                &cwd,
            ],
        );
        assert!(output.status.success(), "{seat}: {}", stderr(&output));
        let script =
            std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/{seat}.sh"))).unwrap();
        let wrapped = script.contains(&format!("'pty' '--' '{provider}' "))
            || script.contains("'pty' '--' 'env' ");
        assert_eq!(wrapped, provider != "codex", "{seat}: {script}");
        assert!(!script.contains("PI_PROGRAM_STATUS"), "{seat}: {script}");
    }
}
