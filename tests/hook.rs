use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-hook-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".swarm")).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// `exe` with only HOME, PATH, and SWARM_HOME set. SWARM_HOME pins the data to HOME, so a branch
/// build (ADR 0027) reads the same place as a `main` one.
fn clean(exe: &Path, home: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env("PATH", "/usr/bin:/bin");
    command
}

/// Runs `command` with `stdin` as its input.
fn piped(mut command: Command, stdin: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// Runs the built binary in a clean env with the given variables added.
fn swarm(home: &Path, env: &[(&str, &str)], args: &[&str], stdin: &str) -> Output {
    let mut command = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), home);
    command
        .env("SWARM_ADAPTER", "tmux")
        .envs(env.iter().copied())
        .current_dir(home)
        .args(args);
    piped(command, stdin)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn coder_state(home: &Path, session: &str) -> serde_json::Value {
    let listing = swarm(
        home,
        &[("SWARM_SESSION_ID", session)],
        &["agents", "--json"],
        "",
    );
    let listing: serde_json::Value = serde_json::from_str(&stdout(&listing)).unwrap();
    let coder = &listing["agents"][0];
    assert_eq!(coder["id"], "coder");
    coder.clone()
}

#[test]
fn a_claude_permission_request_shows_the_coder_waiting() {
    let home = scratch("waiting");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let in_session = [("SWARM_SESSION_ID", session.as_str())];
    let added = swarm(&home, &in_session, &["agent", "add", "coder", "coder"], "");
    assert!(added.status.success());
    let sent_to = stand_in_notify(&home);

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
    assert_eq!(notices(&sent_to).lines().count(), 1);

    let broken = swarm(&home, &as_coder, &["hook", "claude"], "not json");
    assert!(broken.status.success());
    assert_eq!(stdout(&broken), "{}\n");
    assert!(!broken.stderr.is_empty());
}

/// `name` run as a copy of the built binary at its own path, as the brew CLI and the app's helper
/// are two paths on one Mac.
fn build_at(home: &Path, name: &str) -> PathBuf {
    let dir = home.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("swarm");
    std::fs::copy(env!("CARGO_BIN_EXE_swarm"), &exe).unwrap();
    exe
}

fn hooks(exe: &Path, home: &Path, args: &[&str]) -> Output {
    let mut command = clean(exe, home);
    command.arg("hooks").args(args);
    piped(command, "")
}

#[test]
fn two_swarm_builds_share_one_hook_setup_and_keep_the_owners_hooks() {
    let home = scratch("two-builds");
    let brew = build_at(&home, "brew");
    let app = build_at(&home, "app");
    let codex_config = home.join(".codex/config.toml");
    let agy_hooks = home.join(".gemini/config/hooks.json");
    let owners_codex = "model = \"o3\"\n\n[hooks.state.\"/owner/config.toml:stop:0:0\"]\ntrusted_hash = \"sha256:owner\"\n";
    let owners_agy =
        r#"{"herdr": {"Stop": [{"type": "command", "command": "herdr-state", "timeout": 10}]}}"#;
    std::fs::create_dir_all(codex_config.parent().unwrap()).unwrap();
    std::fs::create_dir_all(agy_hooks.parent().unwrap()).unwrap();
    std::fs::write(&codex_config, owners_codex).unwrap();
    std::fs::write(&agy_hooks, owners_agy).unwrap();
    let all_set = serde_json::json!({"codex": true, "agy": true, "guard": true});

    let setup = hooks(&brew, &home, &["setup"]);
    assert!(setup.status.success(), "{setup:?}");
    for build in [&brew, &app] {
        let status = hooks(build, &home, &["status", "--json"]);
        let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
        assert_eq!(status, all_set, "{}", build.display());
    }
    let (codex_after, agy_after) = (
        std::fs::read_to_string(&codex_config).unwrap(),
        std::fs::read_to_string(&agy_hooks).unwrap(),
    );
    let again = hooks(&app, &home, &["setup"]);
    assert!(
        again.status.success() && again.stdout.is_empty(),
        "{again:?}"
    );
    assert_eq!(std::fs::read_to_string(&codex_config).unwrap(), codex_after);
    assert_eq!(std::fs::read_to_string(&agy_hooks).unwrap(), agy_after);

    assert!(codex_after.starts_with(owners_codex), "{codex_after}");
    let agy: serde_json::Value = serde_json::from_str(&agy_after).unwrap();
    let owner: serde_json::Value = serde_json::from_str(owners_agy).unwrap();
    assert_eq!(agy["herdr"], owner["herdr"]);
    for build in [&brew, &app] {
        assert!(!codex_after.contains(&*build.to_string_lossy()));
        assert!(!agy_after.contains(&*build.to_string_lossy()));
    }
    std::fs::remove_dir_all(&home).unwrap();
}

/// Runs a hook command as Codex and AGY do, through the user's shell, with no `swarm` on PATH.
fn run_hook(shell: &str, command: &str, env: &[(&str, &str)], stdin: &str) -> Output {
    let mut hook = Command::new(shell);
    hook.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .envs(env.iter().copied())
        .args(["-c", command]);
    piped(hook, stdin)
}

/// The user shells on this machine that a hook may run in; fish is not in the macOS base system.
fn shells() -> Vec<&'static str> {
    [
        "/bin/sh",
        "/bin/zsh",
        "/bin/csh",
        "/opt/homebrew/bin/fish",
        "/usr/bin/fish",
    ]
    .into_iter()
    .filter(|shell| Path::new(shell).exists())
    .collect()
}

#[test]
fn the_shared_hook_runs_the_panes_own_swarm_and_is_quiet_outside_an_agent() {
    let home = scratch("pane-link");
    let setup = hooks(Path::new(env!("CARGO_BIN_EXE_swarm")), &home, &["setup"]);
    assert!(setup.status.success(), "{setup:?}");
    let agy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join(".gemini/config/hooks.json")).unwrap())
            .unwrap();
    let stop = agy["swarm"]["Stop"][0]["command"].as_str().unwrap();

    for shell in shells() {
        let outside = run_hook(shell, stop, &[], "{}");
        assert!(outside.status.success(), "{shell}: {outside:?}");
        assert_eq!(stdout(&outside), "{}", "{shell}");
        assert!(outside.stderr.is_empty(), "{shell}: {outside:?}");
    }

    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let in_session = [("SWARM_SESSION_ID", session.as_str())];
    assert!(
        swarm(&home, &in_session, &["agent", "add", "coder", "coder"], "")
            .status
            .success()
    );
    // `swarm launch` makes this link for each pane; see `swarm_bin` in main.rs.
    let bin = home.join(format!(".swarm/runs/{session}/bin"));
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(build_at(&home, "launcher"), bin.join("swarm")).unwrap();
    let home_text = home.to_string_lossy();
    let as_coder = [
        ("SWARM_HOME", home_text.as_ref()),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "coder"),
    ];
    let working = agy["swarm"]["PreInvocation"][0]["command"]
        .as_str()
        .unwrap();
    for shell in shells() {
        for (command, state) in [(working, "working"), (stop, "done")] {
            let reported = run_hook(shell, command, &as_coder, "{}");
            assert!(reported.status.success(), "{shell}: {reported:?}");
            assert_eq!(stdout(&reported), "{}\n", "{shell}");
            assert_eq!(coder_state(&home, &session)["state"], state, "{shell}");
        }
    }
    std::fs::remove_dir_all(&home).unwrap();
}

/// Each hook file's bytes, or None for a missing one, so a test can show that nothing changed.
fn hook_files(home: &Path) -> [Option<Vec<u8>>; 2] {
    [".codex/config.toml", ".gemini/config/hooks.json"]
        .map(|file| std::fs::read(home.join(file)).ok())
}

/// The plan shows each line that setup adds and writes nothing; apply with the plan's digest
/// writes, and refuses a file that changed after the plan (ADR 0036).
#[test]
fn the_setup_plan_shows_each_change_and_writes_nothing() {
    let home = scratch("plan");
    let exe = Path::new(env!("CARGO_BIN_EXE_swarm"));
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"o3\"\n").unwrap();
    let before = hook_files(&home);

    let plan = hooks(exe, &home, &["setup", "--plan"]);
    assert!(plan.status.success(), "{plan:?}");
    let text = stdout(&plan);
    let codex = home.join(".codex/config.toml").display().to_string();
    let agy = home.join(".gemini/config/hooks.json").display().to_string();
    for added in [
        format!("+++ {codex}\n@@ -1 +1,"),
        "+[hooks.state.\"/<session-flags>/config.toml:stop:1:0\"]\n".into(),
        format!("+++ {agy}\n@@ -0,0 +1,"),
        "+  \"swarm\": {\n".into(),
    ] {
        assert!(text.contains(&added), "{added}\n{text}");
    }
    assert!(text.ends_with("Plan only. No file written. Run `swarm hooks setup` to apply.\n"));
    assert_eq!(hook_files(&home), before);

    let json = hooks(exe, &home, &["setup", "--plan", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(hook_files(&home), before);
    assert_eq!(json["files"].as_array().unwrap().len(), 2);
    assert_eq!(json["conflicts"], serde_json::json!([]));
    let digest = json["digest"].as_str().unwrap();

    std::fs::write(home.join(".codex/config.toml"), "model = \"o4\"\n").unwrap();
    let changed = hooks(exe, &home, &["setup", "--digest", digest]);
    assert!(!changed.status.success());
    assert!(String::from_utf8_lossy(&changed.stderr).contains("changed after the plan"));
    assert_eq!(
        std::fs::read_to_string(home.join(".codex/config.toml")).unwrap(),
        "model = \"o4\"\n"
    );

    let json = hooks(exe, &home, &["setup", "--plan", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let applied = hooks(
        exe,
        &home,
        &["setup", "--digest", json["digest"].as_str().unwrap()],
    );
    assert!(applied.status.success(), "{applied:?}");
    let status = hooks(exe, &home, &["status", "--json"]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap(),
        serde_json::json!({"codex": true, "agy": true, "guard": true})
    );
    let done = hooks(exe, &home, &["setup", "--plan"]);
    assert_eq!(
        stdout(&done),
        "swarm's hooks are already set up. No file changes.\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// An owner's entry at the place swarm needs, in either provider, is a named conflict: setup
/// leaves every file as it was and fails (ADR 0036).
#[test]
fn an_owners_entry_at_swarms_place_is_a_conflict_and_nothing_is_written() {
    let exe = Path::new(env!("CARGO_BIN_EXE_swarm"));
    let codex_entry = "[hooks.state.\"/<session-flags>/config.toml:stop:1:0\"]\ntrusted_hash = \"sha256:owner\"\n";
    let agy_group = r#"{"swarm": {"Stop": [{"type": "command", "command": "my-own-swarm-tool"}]}}"#;
    for (name, file, text, entry, found) in [
        (
            "codex",
            ".codex/config.toml",
            codex_entry,
            "[hooks.state.\"/<session-flags>/config.toml:stop:1:0\"]",
            "sha256:owner",
        ),
        (
            "agy",
            ".gemini/config/hooks.json",
            agy_group,
            "group \"swarm\"",
            "my-own-swarm-tool",
        ),
    ] {
        let home = scratch(&format!("conflict-{name}"));
        let path = home.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        let before = hook_files(&home);

        let setup = hooks(exe, &home, &["setup"]);
        let stderr = String::from_utf8_lossy(&setup.stderr);
        assert!(!setup.status.success(), "{name}: {setup:?}");
        assert!(
            stderr.contains(&format!("conflict: {} {entry}", path.display()))
                && stderr.contains(found)
                && stderr.contains("1 conflict. No file written."),
            "{name}: {stderr}"
        );
        assert_eq!(hook_files(&home), before, "{name}");

        let plan = hooks(exe, &home, &["setup", "--plan", "--json"]);
        assert!(plan.status.success(), "{name}: {plan:?}");
        let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
        assert_eq!(
            plan["conflicts"][0]["file"],
            path.display().to_string(),
            "{name}"
        );
        assert_eq!(plan["conflicts"][0]["kind"], "taken", "{name}");
        assert!(
            plan["conflicts"][0]["fix"]
                .as_str()
                .unwrap()
                .contains(&path.display().to_string())
        );
        assert_eq!(hook_files(&home), before, "{name}");
        std::fs::remove_dir_all(&home).unwrap();
    }
}

/// A Codex home whose config swarm cannot read is a named conflict in the plan, so setup writes no
/// file and the owner sees the fix (owner's choice, 2026-10-01).
#[test]
fn a_broken_codex_home_is_a_conflict_and_nothing_is_written() {
    let home = scratch("broken-home");
    let exe = Path::new(env!("CARGO_BIN_EXE_swarm"));
    let broken = home.join(".codex-old/config.toml");
    std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
    std::fs::write(&broken, "not toml = =\n").unwrap();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"o3\"\n").unwrap();
    let before = (hook_files(&home), std::fs::read(&broken).unwrap());

    let plan = hooks(exe, &home, &["setup", "--plan", "--json"]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let conflicts = plan["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{plan}");
    assert_eq!(conflicts[0]["file"], broken.display().to_string());
    assert_eq!(conflicts[0]["kind"], "unreadable");
    assert!(
        conflicts[0]["found"]
            .as_str()
            .unwrap()
            .contains("not valid TOML")
    );

    let setup = hooks(exe, &home, &["setup"]);
    assert!(!setup.status.success(), "{setup:?}");
    assert_eq!((hook_files(&home), std::fs::read(&broken).unwrap()), before);
    std::fs::remove_dir_all(&home).unwrap();
}

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
    assert_eq!(
        stdout(&again),
        "swarm's hooks are already set up. No file changes.\n"
    );
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
            "{provider}"
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
        r#"{"rules": [{"name": "no", "event": "PreToolUse", "command": ["/bin/sh", "-c", "echo from elsewhere >&2; exit 2"]}]}"#,
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

fn setup(home: &Path, args: &[&str]) -> Output {
    let mut command = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), home);
    command.current_dir(home).arg("setup").args(args);
    // These fixtures cover the original groups without requiring a bundled skills copy.
    if args.first() != Some(&"status") && !args.contains(&"--only") {
        command.args(["--only", "hooks,trust,herdr"]);
    }
    piped(command, "")
}

fn git_repo(home: &Path, name: &str) -> PathBuf {
    let dir = home.join(name);
    assert!(
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    dir
}

/// One `swarm setup` plan holds every pending write, each file with its group and its diff: the
/// hooks, the consent file, and the trust entries a launch in `--cwd` would write, also in a file
/// that the hooks change too. One digest applies them all, each trust entry is recorded, and a
/// second apply with nothing pending passes (ADR 0043).
#[test]
fn one_setup_plan_holds_every_pending_write_with_a_diff_per_file() {
    let home = scratch("setup-all");
    // A space in the folder must survive the printed apply command.
    let repo = git_repo(&home, "my app");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"o3\"\n").unwrap();
    let cwd = repo.to_string_lossy().into_owned();
    let status = || -> serde_json::Value {
        serde_json::from_slice(&setup(&home, &["status", "--json"]).stdout).unwrap()
    };
    assert_eq!(
        status(),
        serde_json::json!({"hooks": false, "guard": true, "trust": false, "herdr": true, "skills": false})
    );

    let plan = setup(&home, &["--plan", "--json", "--cwd", &cwd]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    // No answer yet, so the plan offers standing consent.
    assert_eq!(plan["consent"], "standing");
    assert_eq!(plan["conflicts"], serde_json::json!([]));
    let files: Vec<(String, String)> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| {
            let path = file["path"].as_str().unwrap();
            let name = path.strip_prefix(&*home.to_string_lossy()).unwrap();
            (
                file["group"].as_str().unwrap().to_string(),
                name.to_string(),
            )
        })
        .collect();
    let expected = [
        ("hooks", "/.codex/config.toml"),
        ("hooks", "/.gemini/config/hooks.json"),
        ("trust", "/.swarm/consent.json"),
        ("trust", "/.codex/config.toml"),
        ("trust", "/.gemini/antigravity-cli/settings.json"),
        ("trust", "/.claude.json"),
    ]
    .map(|(group, path)| (group.to_string(), path.to_string()));
    assert_eq!(files, expected);
    let diffs: Vec<&str> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["diff"].as_str().unwrap())
        .collect();
    assert!(
        diffs[2].contains("+  \"trust\": \"standing\""),
        "{}",
        diffs[2]
    );
    assert!(
        diffs[3].contains(&format!("+[projects.\"{cwd}\"]")),
        "{}",
        diffs[3]
    );
    assert!(!diffs[3].contains("+[hooks.state"), "{}", diffs[3]);
    assert!(
        diffs[5].contains(&format!("{cwd}/.herdr/workers")),
        "{}",
        diffs[5]
    );

    let text = stdout(&setup(&home, &["--plan", "--cwd", &cwd]));
    let digest = plan["digest"].as_str().unwrap();
    assert!(
        text.ends_with(&format!(
            "Plan only. No file written. Run `swarm setup --digest {digest} --cwd '{cwd}' --only hooks,trust,herdr` to apply.\n"
        )),
        "{text}"
    );

    let applied = setup(&home, &["--digest", digest, "--cwd", &cwd]);
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        status(),
        serde_json::json!({"hooks": true, "guard": true, "trust": true, "herdr": true, "skills": false})
    );
    let config = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    assert!(config.contains("[hooks.state.") && config.contains(&format!("[projects.\"{cwd}\"]")));
    let list = swarm(&home, &[], &["managed", "list", "--json"], "");
    let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let trust = list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["writer"] == "launch.trust" && entry["recorded"] == true)
        .count();
    // The consent file, Codex, AGY, and Claude.
    assert_eq!(trust, 4, "{list}");

    let again = setup(&home, &["--digest", digest, "--cwd", &cwd]);
    assert!(again.status.success(), "{again:?}");
    assert!(stdout(&again).contains("already set up"), "{again:?}");

    // `--only` plans one group; a folder that fails the trust check is skipped with its reason.
    let only = setup(
        &home,
        &[
            "--plan",
            "--json",
            "--only",
            "trust",
            "--cwd",
            &*home.to_string_lossy(),
        ],
    );
    let only: serde_json::Value = serde_json::from_slice(&only.stdout).unwrap();
    assert_eq!(only["files"], serde_json::json!([]));
    assert_eq!(only["skipped"][0]["group"], "trust");
    assert!(
        only["skipped"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("not in a git repository"),
        "{only}"
    );
    let unknown = setup(&home, &["--plan", "--only", "sound"]);
    assert!(!unknown.status.success());
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
        &[
            "--digest",
            plan["digest"].as_str().unwrap(),
            "--only",
            "hooks",
        ],
    );
    assert!(
        !applied.status.success(),
        "the digest covers every group: {applied:?}"
    );
    let plan = setup(&home, &["--plan", "--json", "--only", "hooks"]);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let applied = setup(
        &home,
        &[
            "--digest",
            plan["digest"].as_str().unwrap(),
            "--only",
            "hooks",
        ],
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

/// A Codex project the owner set to another trust level is left as it is, and the plan says why
/// the pane will ask (ADR 0043, C3).
#[test]
fn a_codex_folder_the_owner_left_untrusted_is_skipped_with_its_reason() {
    let home = scratch("setup-untrusted");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let config = format!("[projects.\"{cwd}\"]\ntrust_level = \"untrusted\"\n");
    std::fs::write(home.join(".codex/config.toml"), &config).unwrap();

    let plan = setup(
        &home,
        &["--plan", "--json", "--only", "trust", "--cwd", &cwd],
    );
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let codex = home.join(".codex/config.toml");
    assert!(
        plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["path"] != codex.to_string_lossy().as_ref()),
        "{plan}"
    );
    let skipped = &plan["skipped"];
    assert_eq!(skipped.as_array().unwrap().len(), 1, "{plan}");
    assert_eq!(skipped[0]["group"], "trust");
    let reason = skipped[0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("you marked this folder untrusted; the pane asks"),
        "{reason}"
    );
    assert!(reason.contains(&*codex.to_string_lossy()), "{reason}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// A spare Claude profile whose config swarm cannot edit does not block setup: as a launch with no
/// picked account does, setup names it under skipped and plans every other file.
#[test]
fn a_read_only_spare_claude_profile_is_skipped_and_the_rest_is_planned() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("setup-spare-claude");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let spare = home.join(".claude/.profiles/spare/.claude.json");
    std::fs::create_dir_all(spare.parent().unwrap()).unwrap();
    std::fs::write(&spare, "{}\n").unwrap();
    std::fs::set_permissions(&spare, std::fs::Permissions::from_mode(0o444)).unwrap();

    let plan = setup(&home, &["--plan", "--json", "--cwd", &cwd]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["conflicts"], serde_json::json!([]), "{plan}");
    let paths: Vec<&str> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    let main = home.join(".claude.json");
    assert!(paths.contains(&&*main.to_string_lossy()), "{paths:?}");
    assert!(!paths.contains(&&*spare.to_string_lossy()), "{paths:?}");
    let skipped = plan["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{plan}");
    assert_eq!(skipped[0]["group"], "trust");
    let reason = skipped[0]["reason"].as_str().unwrap();
    assert!(reason.contains(&*spare.to_string_lossy()), "{reason}");

    let applied = setup(
        &home,
        &["--digest", plan["digest"].as_str().unwrap(), "--cwd", &cwd],
    );
    assert!(applied.status.success(), "{applied:?}");
    assert!(std::fs::read_to_string(&main).unwrap().contains(&cwd));
    assert_eq!(std::fs::read_to_string(&spare).unwrap(), "{}\n");
    std::fs::set_permissions(&spare, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
}

/// An apply plans the trust group once and each trust write plans again only its own file, so a
/// write under the trust lock runs no second `git rev-parse` (L-1).
#[test]
fn an_apply_checks_the_folder_with_git_once() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("setup-one-git");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let (bin, log) = (home.join("bin"), home.join("git.log"));
    std::fs::create_dir_all(&bin).unwrap();
    let shim = bin.join("git");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{}'\nexec /usr/bin/git \"$@\"\n",
        log.display()
    );
    std::fs::write(&shim, script).unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = |args: &[&str]| {
        let mut command = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), &home);
        let path = format!("{}:/usr/bin:/bin", bin.display());
        command
            .env("PATH", path)
            .current_dir(&home)
            .arg("setup")
            .args(args);
        piped(command, "")
    };

    let plan = run(&["--plan", "--json", "--only", "trust", "--cwd", &cwd]);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert!(plan["files"].as_array().unwrap().len() > 1, "{plan}");
    std::fs::remove_file(&log).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    let applied = run(&["--digest", digest, "--only", "trust", "--cwd", &cwd]);
    assert!(applied.status.success(), "{applied:?}");
    let calls = std::fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().count(), 1, "{calls}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// A Codex config.toml that swarm cannot read is a hooks conflict, and the trust group plans
/// nothing on it, so the owner sees no false diff; it names the conflict instead.
#[test]
fn a_broken_codex_config_gets_no_trust_diff_only_the_reason() {
    let home = scratch("setup-broken-trust");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let codex = home.join(".codex/config.toml");
    std::fs::write(&codex, "model = \"gpt-5.5\"\nbroken = = \n").unwrap();

    let plan = setup(&home, &["--plan", "--json", "--cwd", &cwd]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert!(
        plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["path"] != codex.to_string_lossy().as_ref()),
        "{plan}"
    );
    let conflicts = plan["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{plan}");
    assert_eq!(conflicts[0]["group"], "hooks");
    let skipped = &plan["skipped"][0];
    assert_eq!(skipped["group"], "trust", "{plan}");
    let reason = skipped["reason"].as_str().unwrap();
    assert!(
        reason.contains(&*codex.to_string_lossy()) && reason.contains("not valid TOML"),
        "{reason}"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// A running Claude rewrites `~/.claude.json` at any moment, so the digest of a trust entry covers
/// only the entry the plan adds and the value it replaces: a hand-run apply after another key
/// changed still passes, and a change to the entry itself is refused (ADR 0043, L-4).
#[test]
fn a_trust_digest_covers_only_the_entries_the_plan_writes() {
    let home = scratch("setup-trust-digest");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let claude = home.join(".claude.json");
    std::fs::write(&claude, "{\"numStartups\": 1}\n").unwrap();
    let digest = |home: &Path| -> String {
        let plan = setup(
            home,
            &["--plan", "--json", "--only", "trust", "--cwd", &cwd],
        );
        let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
        plan["digest"].as_str().unwrap().to_string()
    };

    let planned = digest(&home);
    let pool = format!("{cwd}/.herdr/workers");
    let mut value = serde_json::json!({"numStartups": 2});
    value["projects"][&pool] = serde_json::json!({"hasTrustDialogAccepted": false});
    std::fs::write(&claude, value.to_string()).unwrap();
    let refused = setup(
        &home,
        &["--digest", &planned, "--only", "trust", "--cwd", &cwd],
    );
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("a file changed after the plan"),
        "{refused:?}"
    );

    let planned = digest(&home);
    value["numStartups"] = 3.into();
    value["tipsHistory"] = serde_json::json!({"x": 1});
    std::fs::write(&claude, value.to_string()).unwrap();
    let applied = setup(
        &home,
        &["--digest", &planned, "--only", "trust", "--cwd", &cwd],
    );
    assert!(applied.status.success(), "{applied:?}");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude).unwrap()).unwrap();
    assert_eq!(written["projects"][&pool]["hasTrustDialogAccepted"], true);
    assert_eq!(written["numStartups"], 3);
    std::fs::remove_dir_all(&home).unwrap();
}

/// The launch consent is a managed edit: Managed Changes lists it, and its undo of a first answer
/// records `ask`, which counts as set up, so the app does not ask again (ADR 0043, "asks once").
/// `--consent ask` records the owner's ask answer (owner answer 2026-10-06).
#[test]
fn launch_consent_is_a_managed_edit_that_reverts_to_ask() {
    let home = scratch("setup-consent");
    let consent = home.join(".swarm/consent.json");
    let status = || -> serde_json::Value {
        serde_json::from_slice(&setup(&home, &["status", "--json"]).stdout).unwrap()
    };
    let apply = |extra: &[&str]| {
        let mut args = vec!["--plan", "--json", "--only", "trust"];
        args.extend(extra);
        let plan: serde_json::Value = serde_json::from_slice(&setup(&home, &args).stdout).unwrap();
        let mut args = vec![
            "--digest",
            plan["digest"].as_str().unwrap(),
            "--only",
            "trust",
        ];
        args.extend(extra);
        let applied = setup(&home, &args);
        assert!(applied.status.success(), "{applied:?}");
        plan
    };
    let consent_entries = || -> Vec<serde_json::Value> {
        let list = swarm(&home, &[], &["managed", "list", "--json"], "");
        let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
        list["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["path"] == serde_json::json!(["trust"]))
            .cloned()
            .collect()
    };

    apply(&[]);
    assert_eq!(status()["trust"], true);
    let entries = consent_entries();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0]["writer"], "launch.trust");
    assert_eq!(entries[0]["wrote"], "standing");
    assert_eq!(entries[0]["state"], "present");

    let id = entries[0]["id"].as_str().unwrap();
    let plan = swarm(
        &home,
        &[],
        &["managed", "revert", id, "--plan", "--json"],
        "",
    );
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    let reverted = swarm(
        &home,
        &[],
        &["managed", "revert", id, "--digest", digest],
        "",
    );
    assert!(reverted.status.success(), "{reverted:?}");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&consent).unwrap()).unwrap();
    assert_eq!(value["trust"], "ask", "{value}");
    assert_eq!(status()["trust"], true);
    assert_eq!(consent_entries()[0]["state"], "off");

    std::fs::remove_file(&consent).unwrap();
    let plan = apply(&["--consent", "ask"]);
    assert!(
        plan["files"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("+  \"trust\": \"ask\""),
        "{plan}"
    );
    assert_eq!(status()["trust"], true);
    let plan: serde_json::Value =
        serde_json::from_slice(&setup(&home, &["--plan", "--json", "--only", "trust"]).stdout)
            .unwrap();
    assert_eq!(plan["consent"], "ask");
    let bad = setup(&home, &["--plan", "--consent", "always"]);
    assert!(!bad.status.success());
    std::fs::remove_dir_all(&home).unwrap();
}

/// A child pane may read the setup plan, but not apply it: an apply writes the owner's consent
/// and config, so a seat left on `ask` could turn on standing consent itself (SRV-2).
#[test]
fn a_child_agent_may_plan_setup_but_not_apply_it() {
    let home = scratch("setup-child");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let child = [("SWARM_AGENT_ID", "coder"), ("SWARM_SESSION_ID", "s1")];
    let status = swarm(&home, &child, &["setup", "status", "--json"], "");
    assert!(status.status.success(), "{status:?}");
    let plan = swarm(
        &home,
        &child,
        &["setup", "--plan", "--json", "--cwd", &cwd],
        "",
    );
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    for args in [
        vec!["setup", "--consent", "standing", "--cwd", &cwd],
        vec!["setup", "--digest", digest, "--cwd", &cwd],
    ] {
        let applied = swarm(&home, &child, &args, "");
        assert!(!applied.status.success(), "{applied:?}");
        assert!(
            String::from_utf8_lossy(&applied.stderr).contains("a child agent cannot"),
            "{applied:?}"
        );
    }
    assert!(!home.join(".swarm/consent.json").exists());
    assert!(!home.join(".claude.json").exists());
    std::fs::remove_dir_all(&home).unwrap();
}

/// The approve command a seat launch prints under `ask` approves the folder and keeps the ask
/// answer: with no `--consent`, setup plans the owner's recorded answer, and the plan reports the
/// answer it sets, which the app's radio starts at.
#[test]
fn setup_with_no_consent_flag_keeps_the_recorded_answer() {
    let home = scratch("setup-keeps-ask");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let consent = home.join(".swarm/consent.json");
    std::fs::write(&consent, "{\"trust\": \"ask\"}\n").unwrap();
    let plan = setup(
        &home,
        &["--plan", "--json", "--only", "trust", "--cwd", &cwd],
    );
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["consent"], "ask");
    let paths: Vec<&str> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    assert!(
        !paths.iter().any(|path| path.ends_with("consent.json")),
        "{paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with(".claude.json")),
        "{paths:?}"
    );
    let digest = plan["digest"].as_str().unwrap();
    let applied = setup(
        &home,
        &["--digest", digest, "--only", "trust", "--cwd", &cwd],
    );
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(&consent).unwrap(),
        "{\"trust\": \"ask\"}\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// A bare `swarm setup` that waits on the trust lock reads the owner's answer once it holds the
/// lock, so an `ask` that the app records while it waits is kept, not written over with standing
/// (L-4).
#[test]
fn a_setup_waiting_on_the_trust_lock_keeps_an_answer_recorded_meanwhile() {
    let home = scratch("setup-lock-answer");
    let repo = git_repo(&home, "app");
    let consent = home.join(".swarm/consent.json");
    let holder = std::fs::File::create(home.join(".swarm/trust.lock")).unwrap();
    holder.lock().unwrap();
    let waiting = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), &home)
        .current_dir(&home)
        .args(["setup", "--only", "trust", "--cwd"])
        .arg(&repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The waiting setup has read the files it reads before the lock by now.
    std::thread::sleep(std::time::Duration::from_millis(500));
    std::fs::write(&consent, "{\"trust\": \"ask\"}\n").unwrap();
    drop(holder);
    let applied = waiting.wait_with_output().unwrap();
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(&consent).unwrap(),
        "{\"trust\": \"ask\"}\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// Lays a stand-in `notify` verb over the shipped tmux adapter. It appends one line per notice to
/// `$HOME/notices`, so a test never shows a real notice. Call it after swarm has claimed its home.
fn stand_in_notify(home: &Path) -> PathBuf {
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux.conf"),
        "notify = printf '%s|%s\\n' \"$SWARM_TITLE\" \"$SWARM_BODY\" >> \"$HOME/notices\"\n",
    )
    .unwrap();
    home.join("notices")
}

fn notices(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap_or_default()
}

/// A chair started by hand has no swarm env at all, and `swarm notify` still reaches the owner.
#[test]
fn swarm_notify_sends_one_notice_for_a_chair_started_by_hand() {
    let home = scratch("notify-chair");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let sent_to = stand_in_notify(&home);

    let sent = swarm(
        &home,
        &[],
        &[
            "notify",
            "swarm: demo done",
            "--body",
            "it's \"done\" $HOME",
        ],
        "",
    );

    assert!(sent.status.success(), "{sent:?}");
    assert_eq!(notices(&sent_to), "swarm: demo done|it's \"done\" $HOME\n");
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn swarm_notify_refuses_a_worker() {
    let home = scratch("notify-worker");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let sent_to = stand_in_notify(&home);

    let refused = swarm(
        &home,
        &[("SWARM_AGENT_ID", "coder")],
        &["notify", "swarm: blocked"],
        "",
    );

    assert!(!refused.status.success(), "{refused:?}");
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr),
        "swarm: only the chair notifies the owner; send it to the orchestrator with swarm send\n"
    );
    assert_eq!(notices(&sent_to), "");
    std::fs::remove_dir_all(&home).unwrap();
}

fn hold_app_lock(path: &Path) -> std::fs::File {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    let file = std::fs::File::create(path).unwrap();
    // SAFETY: the descriptor stays open in the returned file; 2 | 4 is LOCK_EX | LOCK_NB.
    assert_eq!(unsafe { flock(file.as_raw_fd(), 2 | 4) }, 0);
    file
}

#[test]
fn swarm_notify_skips_a_held_app_lock_and_sends_after_release() {
    let home = scratch("notify-app-held");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let sent_to = stand_in_notify(&home);
    let app_lock = home.join(".swarm/app.lock");
    let held = hold_app_lock(&app_lock);
    std::fs::write(&app_lock, std::process::id().to_string()).unwrap();
    let skipped = swarm(
        &home,
        &[],
        &["notify", "Swarm — chat", "--body", "done"],
        "",
    );
    assert!(skipped.status.success(), "{skipped:?}");
    assert!(skipped.stdout.is_empty() && skipped.stderr.is_empty());
    assert_eq!(notices(&sent_to), "");
    drop(held);
    let sent = swarm(
        &home,
        &[],
        &["notify", "Swarm — chat", "--body", "done"],
        "",
    );
    assert!(sent.status.success(), "{sent:?}");
    assert_eq!(notices(&sent_to), "Swarm — chat|done\n");
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn swarm_notify_sends_for_missing_and_unlocked_pid_files() {
    for (role, content) in [
        ("missing", None),
        ("recycled-live-pid", Some(std::process::id().to_string())),
        ("garbage", Some("not a pid".into())),
    ] {
        let home = scratch(&format!("notify-app-{role}"));
        assert!(swarm(&home, &[], &["init"], "").status.success());
        let sent_to = stand_in_notify(&home);
        if let Some(content) = content {
            std::fs::write(home.join(".swarm/app.lock"), content).unwrap();
        }
        let output = swarm(
            &home,
            &[],
            &["notify", "Swarm — chat", "--body", "done"],
            "",
        );
        assert!(output.status.success(), "{role}: {output:?}");
        assert_eq!(notices(&sent_to), "Swarm — chat|done\n", "{role}");
        std::fs::remove_dir_all(&home).unwrap();
    }
}

#[test]
fn swarm_notify_names_the_missing_verb() {
    let home = scratch("notify-no-verb");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    std::fs::write(
        home.join(".swarm/adapters/bare.conf"),
        "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();

    let failed = swarm(
        &home,
        &[("SWARM_ADAPTER", "bare")],
        &["notify", "swarm: demo done"],
        "",
    );

    assert!(!failed.status.success(), "{failed:?}");
    assert_eq!(
        String::from_utf8_lossy(&failed.stderr),
        "swarm: adapter bare has no notify verb, so it cannot notify the owner\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// Each shipped adapter's `notify` verb gives osascript the title and body as argv, never as
/// AppleScript source. A stand-in `osascript` first on PATH records its args, so no notice shows.
#[test]
fn each_shipped_notify_verb_passes_the_text_to_osascript_as_argv() {
    let home = scratch("notify-osascript");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let osascript = bin.join("osascript");
    std::fs::write(
        &osascript,
        "#!/bin/sh\nfor arg; do printf '<%s>' \"$arg\"; done >> \"$HOME/osascript\"\necho >> \"$HOME/osascript\"\n",
    )
    .unwrap();
    std::fs::set_permissions(
        &osascript,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let title = "swarm: a\" & (do shell script \"touch pwned\") & \"";
    let body = "body $HOME `id`";

    for adapter in ["tmux", "tmux-solo", "herdr"] {
        let sent = swarm(
            &home,
            &[("SWARM_ADAPTER", adapter), ("PATH", &path)],
            &["notify", title, "--body", body],
            "",
        );
        assert!(sent.status.success(), "{adapter}: {sent:?}");
    }

    let line = format!(
        "<-e><on run argv><-e><display notification (item 2 of argv) with title (item 1 of argv) sound name \"Glass\"><-e><end run><{title}><{body}>\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("osascript")).unwrap(),
        line.repeat(3)
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// Claude sends `PermissionRequest` and then `Notification permission_prompt` for one prompt. The
/// owner gets one notice for the change to `waiting`, and the next change to `waiting` gets one more.
#[test]
fn a_change_to_waiting_sends_one_notice_and_a_repeat_sends_none() {
    let home = scratch("notify-waiting");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let in_session = [("SWARM_SESSION_ID", session.as_str())];
    assert!(
        swarm(&home, &in_session, &["agent", "add", "coder", "coder"], "")
            .status
            .success()
    );
    let sent_to = stand_in_notify(&home);
    let as_coder = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "coder"),
    ];
    let hook = |payload: &str| {
        let reported = swarm(&home, &as_coder, &["hook", "claude"], payload);
        assert_eq!(stdout(&reported), "{}\n", "{reported:?}");
    };
    let project = home.file_name().unwrap().to_string_lossy().into_owned();
    let notice =
        format!("swarm: coder needs you|{project}: waiting on a permission or a question\n");

    hook(r#"{"hook_event_name":"PermissionRequest"}"#);
    assert_eq!(notices(&sent_to), notice);

    hook(r#"{"hook_event_name":"Notification","notification_type":"permission_prompt"}"#);
    assert_eq!(notices(&sent_to), notice);

    hook(r#"{"hook_event_name":"PreToolUse"}"#);
    hook(r#"{"hook_event_name":"PermissionRequest"}"#);
    assert_eq!(notices(&sent_to), notice.repeat(2));

    let app_lock = home.join(".swarm/app.lock");
    let held = hold_app_lock(&app_lock);
    hook(r#"{"hook_event_name":"PreToolUse"}"#);
    hook(r#"{"hook_event_name":"PermissionRequest"}"#);
    assert_eq!(notices(&sent_to), notice.repeat(2));
    drop(held);
    hook(r#"{"hook_event_name":"PreToolUse"}"#);
    hook(r#"{"hook_event_name":"PermissionRequest"}"#);
    assert_eq!(notices(&sent_to), notice.repeat(3));
    std::fs::remove_dir_all(&home).unwrap();
}

/// An agent that `swarm close` removed, or one the session never held, has no row to change, so
/// its hook sends no notice however often it reports `waiting`.
#[test]
fn a_hook_for_an_agent_the_session_does_not_hold_sends_no_notice() {
    let home = scratch("notify-ghost");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let sent_to = stand_in_notify(&home);
    let as_ghost = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "ghost"),
    ];

    for _ in 0..2 {
        let reported = swarm(
            &home,
            &as_ghost,
            &["hook", "claude"],
            r#"{"hook_event_name":"PermissionRequest"}"#,
        );
        assert_eq!(stdout(&reported), "{}\n", "{reported:?}");
    }

    assert_eq!(notices(&sent_to), "");
    std::fs::remove_dir_all(&home).unwrap();
}
