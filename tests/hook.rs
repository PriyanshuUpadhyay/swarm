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
    let all_set = serde_json::json!({"codex": true, "agy": true});

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
/// writes, and refuses a file that changed after the plan (ADR 0035).
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
        format!("+++ {codex}\n@@ -1,1 +1,"),
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
        serde_json::json!({"codex": true, "agy": true})
    );
    let done = hooks(exe, &home, &["setup", "--plan"]);
    assert_eq!(
        stdout(&done),
        "swarm's hooks are already set up. No file changes.\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// An owner's entry at the place swarm needs, in either provider, is a named conflict: setup
/// leaves every file as it was and fails (ADR 0035).
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
