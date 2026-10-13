use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-hook-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".swarm")).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("bin/open"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(
        dir.join("bin/open"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
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
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        );
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
    // A hook that is no swarm agent exits before it reads stdin, so the write may hit EPIPE.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
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

/// A Codex home that swarm allocated under its own folder is a Codex home for setup too.
#[test]
fn a_codex_home_under_swarm_accounts_is_planned_like_a_home_folder_one() {
    let home = scratch("swarm-codex-home");
    let exe = Path::new(env!("CARGO_BIN_EXE_swarm"));
    let broken = home.join(".swarm/accounts/codex/work/config.toml");
    std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
    std::fs::write(&broken, "not toml = =\n").unwrap();

    let plan = hooks(exe, &home, &["setup", "--plan", "--json"]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let conflicts = plan["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{plan}");
    assert_eq!(conflicts[0]["file"], broken.display().to_string());
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

/// Waits up to 1 s for the stand-in `open` to record its arguments.
fn opened(home: &Path) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !home.join("opened").exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::fs::read_to_string(home.join("opened")).ok()
}

/// The `open` arguments that start the notice helper at `helper` for this test's home (ADR 0077).
fn helper_start(home: &Path, helper: &Path) -> String {
    format!(
        "-g {} --env SWARM_HOME={} --env SWARM_BIN={}\n",
        helper.display(),
        home.display(),
        env!("CARGO_BIN_EXE_swarm")
    )
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
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/opened\"\nsleep 2\n",
    )
    .unwrap();
    let started = std::time::Instant::now();
    assert!(swarm(&home, &env, &["notify", "Done"], "").status.success());
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(opened(&home).unwrap(), helper_start(&home, &helper));
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
    assert_eq!(opened(&home), None);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn focus_uses_the_stored_adapter_and_pane_outside_a_session() {
    let home = scratch("focus-host");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for host in ["herdr", "tmux"] {
        std::fs::write(
            home.join(format!(".swarm/adapters/{host}.conf")),
            "client = true\n",
        )
        .unwrap();
        let executable = bin.join(host);
        std::fs::write(
            &executable,
            "#!/bin/sh\nfor arg; do printf '<%s>' \"$arg\"; done >> \"$HOME/focus\"\necho >> \"$HOME/focus\"\n",
        )
        .unwrap();
        std::fs::set_permissions(
            executable,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
    }
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    for (adapter, pane, expected) in [
        ("herdr", "wK:p2", "<agent><focus><wK:p2>\n"),
        (
            "tmux",
            "%2",
            "<select-window><-t><%2>\n<select-pane><-t><%2>\n",
        ),
    ] {
        let created = swarm(&home, &[], &["session", "new", "lane"], "");
        assert!(created.status.success(), "{created:?}");
        let session = stdout(&created).trim().to_string();
        let added = swarm(
            &home,
            &[("SWARM_SESSION_ID", &session)],
            &["agent", "add", "coder", "coder"],
            "",
        );
        assert!(added.status.success(), "{added:?}");
        swarm::store::set_adapter(&connection, &session, adapter).unwrap();
        swarm::store::set_pane(&connection, &session, "coder", pane).unwrap();
        let focused = swarm(
            &home,
            &[("PATH", &path), ("SWARM_ADAPTER", "wrong")],
            &["focus", &session, "coder"],
            "",
        );
        assert!(focused.status.success(), "{adapter}: {focused:?}");
        assert_eq!(
            std::fs::read_to_string(home.join("focus")).unwrap(),
            expected
        );
        std::fs::remove_file(home.join("focus")).unwrap();
    }
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn focus_names_missing_panes_adapters_and_verbs() {
    let home = scratch("focus-errors");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let session = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
    swarm::store::add_agent(&connection, &session, "coder", "coder").unwrap();
    let focused = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert!(!focused.status.success(), "{focused:?}");
    assert_eq!(
        String::from_utf8_lossy(&focused.stderr),
        "swarm: session has no adapter\n"
    );
    for adapter in ["tmux-solo", "superset"] {
        swarm::store::set_adapter(&connection, &session, adapter).unwrap();
        let missing = swarm(&home, &[], &["focus", &session, "coder"], "");
        assert!(!missing.status.success(), "{missing:?}");
        assert_eq!(
            String::from_utf8_lossy(&missing.stderr),
            format!("swarm: {adapter} has no focus\n")
        );
        swarm::store::set_pane(&connection, &session, "coder", "%2").unwrap();
        let missing = swarm(&home, &[], &["focus", &session, "coder"], "");
        assert!(!missing.status.success(), "{missing:?}");
        assert_eq!(
            String::from_utf8_lossy(&missing.stderr),
            format!("swarm: {adapter} has no focus\n")
        );
        swarm::store::clear_pane(&connection, &session, "coder").unwrap();
    }
    std::fs::remove_dir_all(&home).unwrap();
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
    for event in [
        "PermissionRequest",
        "PermissionRequest",
        "PreToolUse",
        "PermissionRequest",
    ] {
        let reported = swarm(
            &home,
            &as_coder,
            &["hook", "claude"],
            &format!(r#"{{"hook_event_name":"{event}"}}"#),
        );
        assert_eq!(stdout(&reported), "{}\n", "{reported:?}");
    }
    assert_eq!(coder_state(&home, &session)["state"], "waiting");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while opened(&home).unwrap_or_default().lines().count() < 2
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        opened(&home).unwrap(),
        helper_start(&home, &helper).repeat(2)
    );
    let rows: serde_json::Value =
        serde_json::from_str(&stdout(&swarm(&home, &[], &["notices", "--json"], ""))).unwrap();
    let folder = home.file_name().unwrap().to_str().unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(row["session_id"], session.as_str());
        assert_eq!(row["agent_id"], "coder");
        assert_eq!(row["title"], "Swarm");
        assert_eq!(row["body"], format!("{folder}: waiting on a permission"));
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
    swarm::store::set_state(
        &connection,
        &in_open,
        "coder",
        "waiting",
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
    let add = |session: Option<&str>, agent: Option<&str>, title: &str| {
        swarm::store::add_notice(&connection, session, agent, title, "").unwrap();
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
    add(None, None, "done");
    assert_eq!(due(), [("waiting".to_string(), false)]);
    assert_eq!(due(), []);

    choices(r#"{"post":false}"#);
    add(Some(&in_open), Some("coder"), "waiting");
    assert_eq!(due(), []);

    std::fs::remove_file(home.join(".swarm/choices.json")).unwrap();
    add(Some(&in_muted), Some("coder"), "default");
    assert_eq!(due(), [("default".to_string(), true)]);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn focus_retries_the_chair_and_raises_the_clients_app() {
    let home = scratch("focus-chair");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let session =
        swarm::store::create_session(&connection, "lane", &home, None, Some("herdr")).unwrap();
    for (agent, role, pane) in [
        ("coder", "coder", "closed"),
        ("chair", "orchestrator", "chair-pane"),
    ] {
        swarm::store::add_agent(&connection, &session, agent, role).unwrap();
        swarm::store::set_pane(&connection, &session, agent, pane).unwrap();
    }
    std::fs::write(
        home.join(".swarm/adapters/herdr.conf"),
        "client = printf 101\n",
    )
    .unwrap();
    for (name, script) in [
        (
            "herdr",
            "#!/bin/sh\necho \"$3\" >> \"$HOME/focus\"\ntest \"$3\" = chair-pane\n",
        ),
        (
            "ps",
            "#!/bin/sh\ncase \"$2:$4\" in comm=:101) echo herdr;; ppid=:101) echo 102;; comm=:102) echo zsh;; ppid=:102) echo 103;; comm=:103) echo login;; ppid=:103) echo 104;; comm=:104) echo '/Applications/Terminal Host.app/Contents/MacOS/ghostty';; *) exit 1;; esac\n",
        ),
        (
            "open",
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$HOME/raised\"\n",
        ),
    ] {
        let path = home.join("bin").join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }
    let focused = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert!(focused.status.success(), "{focused:?}");
    assert_eq!(
        std::fs::read_to_string(home.join("focus")).unwrap(),
        "closed\nchair-pane\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("raised")).unwrap(),
        "/Applications/Terminal Host.app\n"
    );
    swarm::store::clear_pane(&connection, &session, "coder").unwrap();
    assert!(
        swarm(&home, &[], &["focus", &session, "coder"], "")
            .status
            .success()
    );
    std::fs::remove_file(home.join("raised")).unwrap();
    std::fs::write(
        home.join("bin/ps"),
        "#!/bin/sh\ncase \"$2\" in comm=) echo herdr;; ppid=) echo 1;; esac\n",
    )
    .unwrap();
    assert!(
        swarm(&home, &[], &["focus", &session, "coder"], "")
            .status
            .success()
    );
    assert!(!home.join("raised").exists());
    swarm::store::clear_pane(&connection, &session, "chair").unwrap();
    let gone = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert_eq!(gone.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&gone.stderr),
        "swarm: pane closed\n"
    );
    std::fs::remove_dir_all(home).unwrap();
}
