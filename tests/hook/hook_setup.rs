// Tests for hook setup.
use super::*;

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
        again.status.success() && stdout(&again) == "Swarm is already set up. Nothing to do.\n",
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
    assert!(
        text.contains("Plan only. Nothing changed. Run `swarm setup hooks --digest "),
        "{text}"
    );
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
    assert_eq!(stdout(&done), "Swarm is already set up. Nothing to do.\n");
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
