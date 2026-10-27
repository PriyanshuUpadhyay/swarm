// Tests for guards.
// Fixture rules use an explicit timeout to check routing under load, within the runner's deadline.
use super::*;

/// With a rule list, setup registers `swarm guard` for Claude, Codex, and AGY next to the owner's
/// own hooks, trusts the Codex group where it lands, and each registered command answers its CLI
/// (ADR 0040). Without a list, setup adds no guard.
#[test]
fn a_rule_list_registers_the_guard_on_every_cli_and_each_registration_answers() {
    let home = scratch("guard");
    let exe = Path::new(env!("CARGO_BIN_EXE_swarm"));
    let write = |file: &str, text: &str| {
        let path = home.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let owner_group = r#"{"matcher": "Bash", "hooks": [{"type": "command", "command": "owner-guard", "timeout": 3}]}"#;
    write(
        ".claude/settings.json",
        &format!(r#"{{"hooks": {{"PreToolUse": [{owner_group}]}}}}"#),
    );
    write(
        ".codex/hooks.json",
        &format!(r#"{{"hooks": {{"PreToolUse": [{owner_group}]}}}}"#),
    );
    write(".codex/config.toml", "model = \"o3\"\n");
    write(".codex-spare/config.toml", "");
    std::fs::create_dir_all(home.join(".claude/.profiles/work")).unwrap();
    std::os::unix::fs::symlink(
        "../../settings.json",
        home.join(".claude/.profiles/work/settings.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink("../.codex/hooks.json", home.join(".codex-spare/hooks.json"))
        .unwrap();
    // Two links to a hooks.json that does not exist yet are still one file.
    write(".codex-fresh/config.toml", "");
    write(".codex-fresh2/config.toml", "");
    std::fs::create_dir_all(home.join("shared")).unwrap();
    for fresh in [".codex-fresh", ".codex-fresh2"] {
        std::os::unix::fs::symlink("../shared/hooks.json", home.join(fresh).join("hooks.json"))
            .unwrap();
    }

    let plan = hooks(exe, &home, &["setup", "--plan"]);
    assert!(!stdout(&plan).contains("swarm guard"), "{}", stdout(&plan));

    write(
        ".swarm/guards.json",
        r#"{"rules": [{"name": "no-blocked", "event": "PreToolUse", "tools": ["Bash", "run_command"],
            "timeout": 30,
            "command": ["/bin/sh", "-c", "grep -q blocked && { echo 'blocked by the rule' >&2; exit 2; }; exit 0"]}]}"#,
    );
    let applied = hooks(exe, &home, &["setup"]);
    assert!(applied.status.success(), "{applied:?}");
    let read = |file: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(home.join(file)).unwrap()).unwrap()
    };
    let guard = |provider: &str| serde_json::json!({"type": "command", "command": format!("swarm guard {provider} PreToolUse"), "timeout": 10});
    let owner: serde_json::Value = serde_json::from_str(owner_group).unwrap();
    for (file, provider) in [
        (".claude/settings.json", "claude"),
        (".codex/hooks.json", "codex"),
    ] {
        assert_eq!(
            read(file)["hooks"]["PreToolUse"],
            serde_json::json!([owner, {"hooks": [guard(provider)]}]),
            "{file}"
        );
    }
    assert!(home.join(".codex-spare/hooks.json").is_symlink());
    assert_eq!(
        read("shared/hooks.json")["hooks"]["PreToolUse"],
        serde_json::json!([{"hooks": [guard("codex")]}])
    );
    assert_eq!(
        read(".gemini/config/hooks.json")["swarm-guard"],
        serde_json::json!({"PreToolUse": [{"matcher": "*", "hooks": [guard("agy")]}]})
    );
    // `codex app-server` 0.159.0 `hooks/list` on 2026-10-05 gave this key and hash for this
    // hooks.json.
    for codex_home in [".codex", ".codex-spare"] {
        let config = std::fs::read_to_string(home.join(codex_home).join("config.toml")).unwrap();
        let key = format!(
            "{}/{codex_home}/hooks.json:pre_tool_use:1:0",
            home.display()
        );
        assert!(
            config.contains(&format!("[hooks.state.{key:?}]\ntrusted_hash = \"sha256:41310acd2af3e803e7ebfd8b6d36709e689a600a2c937f4266953a446bda5e84\"")),
            "{config}"
        );
    }
    let status = hooks(exe, &home, &["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["guard"], true, "{status}");
    let again = hooks(exe, &home, &["setup", "--plan"]);
    assert_eq!(stdout(&again), "Swarm is already set up. Nothing to do.\n");
    // A guard handler with too short a timeout is a conflict, and status says it is not set up.
    let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
    std::fs::write(
        home.join(".claude/settings.json"),
        settings.replace("\"timeout\": 10", "\"timeout\": 3"),
    )
    .unwrap();
    let status = hooks(exe, &home, &["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["guard"], false, "{status}");
    std::fs::write(home.join(".claude/settings.json"), settings).unwrap();
    // With the list gone, the registrations left behind would block every call, so status says so.
    let list = std::fs::read(home.join(".swarm/guards.json")).unwrap();
    std::fs::remove_file(home.join(".swarm/guards.json")).unwrap();
    let status = hooks(exe, &home, &["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["guard"], false, "{status}");
    std::fs::write(home.join(".swarm/guards.json"), list).unwrap();

    // Each registered command, with `swarm` on PATH being this build, answers in its CLI's form.
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(exe, bin.join("swarm")).unwrap();
    let registered = |provider: &str| match provider {
        "agy" => {
            read(".gemini/config/hooks.json")["swarm-guard"]["PreToolUse"][0]["hooks"][0]["command"]
                .clone()
        }
        "codex" => {
            read(".codex/hooks.json")["hooks"]["PreToolUse"][1]["hooks"][0]["command"].clone()
        }
        _ => read(".claude/settings.json")["hooks"]["PreToolUse"][1]["hooks"][0]["command"].clone(),
    };
    let run = |provider: &str, payload: &str| {
        let mut command = clean(Path::new("/bin/sh"), &home);
        command
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .args(["-c", registered(provider).as_str().unwrap()]);
        piped(command, payload)
    };
    let bash = |command: &str| {
        format!(r#"{{"tool_name": "Bash", "tool_input": {{"command": "{command}"}}}}"#)
    };
    let agy = |command: &str| {
        format!(
            r#"{{"toolCall": {{"name": "run_command", "args": {{"CommandLine": "{command}"}}}}}}"#
        )
    };
    for provider in ["claude", "codex"] {
        let allowed = run(provider, &bash("ls"));
        assert_eq!(
            (allowed.status.code(), stdout(&allowed)),
            (Some(0), String::new()),
            "{provider}: {allowed:?}"
        );
    }
    let denied = run("claude", &bash("echo blocked"));
    assert_eq!(
        stdout(&denied),
        "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"blocked by the rule\"}}\n"
    );
    let denied = run("codex", &bash("echo blocked"));
    assert_eq!(denied.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&denied.stderr),
        "blocked by the rule\n"
    );
    assert_eq!(
        stdout(&run("agy", &agy("ls"))),
        "{\"decision\":\"allow\"}\n"
    );
    assert_eq!(
        stdout(&run("agy", &agy("echo blocked"))),
        "{\"decision\":\"deny\",\"reason\":\"blocked by the rule\"}\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// `SWARM_GUARDS` names the rule list in place of `~/.swarm/guards.json`, for every build.
#[test]
fn swarm_guards_names_the_rule_list() {
    let home = scratch("guards-env");
    let list = home.join("elsewhere.json");
    std::fs::write(
        &list,
        r#"{"rules": [{"name": "no", "event": "PreToolUse", "timeout": 30, "command": ["/bin/sh", "-c", "echo from elsewhere >&2; exit 2"]}]}"#,
    )
    .unwrap();
    let payload = r#"{"tool_name": "Bash", "tool_input": {"command": "ls"}}"#;
    let list_text = list.to_string_lossy();
    let denied = swarm(
        &home,
        &[("SWARM_GUARDS", list_text.as_ref())],
        &["guard", "codex", "PreToolUse"],
        payload,
    );
    assert_eq!(denied.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&denied.stderr), "from elsewhere\n");
    let missing = swarm(&home, &[], &["guard", "codex", "PreToolUse"], payload);
    assert!(String::from_utf8_lossy(&missing.stderr).contains(".swarm/guards.json is missing"));
    std::fs::remove_dir_all(&home).unwrap();
}

/// Herdr's own state hooks and an owner's hook sit next to swarm's in every hook file, and swarm's
/// groups, which only swarm names, meet none of them: the plan has no conflict (ADR 0043 rule 1-2).
#[test]
fn herdrs_state_hooks_and_an_owner_hook_give_no_conflict_for_swarms_groups() {
    let home = scratch("herdr-side");
    let write = |file: &str, text: &str| {
        let path = home.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let herdr =
        r#"{"type": "command", "command": "~/.config/herdr/herdr-agent-state.sh", "timeout": 5}"#;
    let owner = r#"{"type": "command", "command": "owner-check", "timeout": 3}"#;
    let groups = format!(
        r#"{{"hooks": {{"PreToolUse": [{{"hooks": [{herdr}]}}, {{"matcher": "Bash", "hooks": [{owner}]}}],
            "Stop": [{{"hooks": [{herdr}]}}]}}}}"#
    );
    write(".claude/settings.json", &groups);
    write(".codex/hooks.json", &groups);
    write(".codex/config.toml", "model = \"o3\"\n");
    write(
        ".gemini/config/hooks.json",
        &format!(
            r#"{{"herdr": {{"Stop": [{herdr}]}}, "agent-harness": {{"PreToolUse": [{owner}]}}}}"#
        ),
    );
    write(".swarm/guards.json", r#"{"rules": []}"#);

    let plan = setup(&home, &["--plan", "--json"]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["conflicts"], serde_json::json!([]), "{plan}");
    let applied = setup(
        &home,
        &["hooks", "--digest", plan["digest"].as_str().unwrap()],
    );
    assert!(
        !applied.status.success(),
        "the digest covers every group: {applied:?}"
    );
    let plan = setup(&home, &["hooks", "--plan", "--json"]);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let applied = setup(
        &home,
        &["hooks", "--digest", plan["digest"].as_str().unwrap()],
    );
    assert!(applied.status.success(), "{applied:?}");
    let agy: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".gemini/config/hooks.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        agy["herdr"]["Stop"][0]["command"],
        "~/.config/herdr/herdr-agent-state.sh"
    );
    std::fs::remove_dir_all(&home).unwrap();
}
