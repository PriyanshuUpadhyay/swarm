//! Pi's launch context and extension set.

use super::*;

/// pi has no hooks: its chair is known at launch from the session id swarm gives it, it reads the
/// host contract from its system prompt, and it reports its state by OSC 7501 (ADR 0086).
#[test]
fn a_pi_chair_is_named_at_launch_and_gets_the_host_contract() {
    let home = scratch("pi-chair");
    tool(&home, "pi", "true");
    let pi_dir = home.join("pi config");
    let bridge = pi_dir.join("npm/node_modules/pi-claude-bridge/src/index.ts");
    let guard = pi_dir.join("extensions/command-guard.ts");
    for path in [&bridge, &guard, &pi_dir.join("extensions/unrelated.ts")] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }
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
    let env = [
        env.as_slice(),
        &[("PI_CODING_AGENT_DIR", pi_dir.to_str().unwrap())],
    ]
    .concat();
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
    assert_eq!(script.matches("[agent-runtime: pi]").count(), 1, "{script}");
    assert!(script.contains("run-workers/SKILL.md"), "{script}");
    assert!(script.contains("'--no-extensions'"), "{script}");
    assert!(!script.contains("'--no-skills'"), "{script}");
    for path in [&bridge, &guard] {
        assert!(
            script.contains(&format!("'--extension' '{}'", path.display())),
            "{script}"
        );
    }
    assert!(script.contains("'--swarm-live'"), "{script}");
    assert_eq!(script.matches("'--extension'").count(), 3, "{script}");
    // This adapter's spawn line names no `$SWARM_FEED`, so pi runs bare.
    assert!(!script.contains("'pty'"), "{script}");

    let sessions = swarm(&home, &[], &["sessions", "--json"]);
    let sessions: serde_json::Value = serde_json::from_slice(&sessions.stdout).unwrap();
    let row = &sessions["sessions"][0];
    assert_eq!(row["chair_provider"], "pi");
    assert_eq!(row["chair_id"], id.as_str());
}

#[test]
fn a_pi_version_outside_the_supported_range_warns_in_doctor_and_launch() {
    let home = scratch("pi-version");
    tool(
        &home,
        "pi",
        "if [ \"$1\" = '--version' ]; then echo '1.6.0'; fi",
    );
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    ).unwrap();
    let doctor = swarm(&home, &[], &["doctor", "--json"]);
    assert!(doctor.status.success(), "{}", stderr(&doctor));
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let items = report["items"].as_array().unwrap();
    let pi = items
        .iter()
        .find(|item| item["id"] == "pi-version")
        .unwrap();
    assert_eq!(pi["state"], "broken", "{pi}");
    assert!(pi["reason"].as_str().unwrap().contains("1.6.0"), "{pi}");
    assert!(pi["reason"].as_str().unwrap().contains("1.5.x"), "{pi}");
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout).unwrap();
    let launch = swarm(
        &home,
        &[app[0], ("SWARM_SESSION_ID", session.trim())],
        &[
            "launch",
            "orchestrator",
            "chat",
            "--provider",
            "pi",
            "--model",
            "anthropic/claude-haiku-5-5",
        ],
    );
    assert!(launch.status.success(), "{}", stderr(&launch));
    assert!(
        stderr(&launch).contains(pi["reason"].as_str().unwrap()),
        "{}",
        stderr(&launch)
    );
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
    assert!(
        stderr(&output).contains("swarm setup pi"),
        "{}",
        stderr(&output)
    );
    let script =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/pi-1.sh"))).unwrap();
    assert_eq!(script.matches("[agent-runtime: pi]").count(), 1, "{script}");
    assert!(
        script.contains(&format!(
            "You are seat `pi-1` with role `pi.seat` in swarm session `{session}`."
        )),
        "{script}"
    );
}
