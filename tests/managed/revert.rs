// Tests for managed change reverts.
use super::*;

/// Done-when 1: a revert of one Codex trust key removes only that key, and the owner's own key in
/// the same `hooks.state` table stays byte for byte. Revert of all gives the owner's file back.
#[test]
fn a_revert_removes_only_swarms_key_and_keeps_the_owners_key_in_the_same_table() {
    let home = scratch("revert-one");
    let owners = "model = \"o3\"\n\n[hooks.state.\"/owner/config.toml:stop:0:0\"]\ntrusted_hash = \"sha256:owner\"\n";
    write(&home, ".codex/config.toml", owners);
    assert!(swarm(&home, &["setup", "hooks", "--yes"]).status.success());
    let codex = home.join(".codex/config.toml");
    let stop = stop_entry(&listing(&home));
    let id = stop["id"].as_str().unwrap();
    let set_up = std::fs::read_to_string(&codex).unwrap();

    let plan = swarm(&home, &["managed", "revert", id, "--plan"]);
    assert!(plan.status.success(), "{plan:?}");
    let plan = String::from_utf8_lossy(&plan.stdout).into_owned();
    assert!(
        plan.contains("-[hooks.state.\"/<session-flags>/config.toml:stop:1:0\"]\n"),
        "{plan}"
    );
    assert!(
        plan.ends_with(&format!(
            "Plan only. No file written. Run `swarm managed revert {id}` to apply.\n"
        )),
        "{plan}"
    );
    assert_eq!(std::fs::read_to_string(&codex).unwrap(), set_up);

    let revert = swarm(&home, &["managed", "revert", id]);
    assert!(revert.status.success(), "{revert:?}");
    assert_eq!(
        String::from_utf8_lossy(&revert.stdout),
        format!("swarm: removed swarm's entries from {}\n", codex.display())
    );
    let after = std::fs::read_to_string(&codex).unwrap();
    let stop_table = format!(
        "[hooks.state.\"/<session-flags>/config.toml:stop:1:0\"]\ntrusted_hash = \"{}\"\n",
        stop["wrote"].as_str().unwrap()
    );
    assert!(set_up.contains(&stop_table));
    assert_eq!(after, set_up.replacen(&format!("\n{stop_table}"), "", 1));
    assert!(after.starts_with(owners), "{after}");
    let entries = listing(&home);
    assert_eq!(stop_entry(&entries)["state"], "off");
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry["state"] == "present")
            .count(),
        6
    );

    let all = swarm(&home, &["managed", "revert", "--all"]);
    assert!(all.status.success(), "{all:?}");
    assert_eq!(std::fs::read_to_string(&codex).unwrap(), owners);
    let again = swarm(&home, &["managed", "revert", "--all", "--plan"]);
    assert_eq!(
        String::from_utf8_lossy(&again.stdout),
        "Swarm has nothing to remove. No file changes.\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// Done-when 2: revert refuses an item someone changed after swarm wrote it, names the file, the
/// key, the value found, and the fix, and writes no file.
#[test]
fn a_revert_refuses_an_item_the_owner_changed_and_names_it() {
    let home = scratch("revert-changed");
    assert!(swarm(&home, &["setup", "hooks", "--yes"]).status.success());
    let codex = home.join(".codex/config.toml");
    let stop = stop_entry(&listing(&home));
    let id = stop["id"].as_str().unwrap();
    let edited = std::fs::read_to_string(&codex)
        .unwrap()
        .replace(stop["wrote"].as_str().unwrap(), "sha256:edited");
    std::fs::write(&codex, &edited).unwrap();

    let revert = swarm(&home, &["managed", "revert", id]);
    assert!(!revert.status.success());
    let text = stderr(&revert);
    for part in [
        format!(
            "conflict: {} [hooks.state.\"/<session-flags>/config.toml:stop:1:0\"] trusted_hash\n",
            codex.display()
        ),
        "  found:  \"sha256:edited\"\n".into(),
        format!("  wanted: \"{}\"\n", stop["wrote"].as_str().unwrap()),
        "  fix:    swarm leaves it; delete".into(),
        "1 conflict. No file written.".into(),
    ] {
        assert!(text.contains(&part), "{part}\n{text}");
    }
    assert_eq!(std::fs::read_to_string(&codex).unwrap(), edited);
    let changed = swarm(&home, &["managed", "revert", id, "--plan", "--json"]);
    let changed: serde_json::Value = serde_json::from_slice(&changed.stdout).unwrap();
    assert_eq!(changed["conflicts"][0]["kind"], "changed", "{changed}");

    // Nothing else of swarm's is touched by --all, and an unknown id is an error.
    let plan = swarm(&home, &["managed", "revert", "--all", "--plan", "--json"]);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["conflicts"], serde_json::json!([]));
    assert_eq!(plan["files"].as_array().unwrap().len(), 2);
    std::fs::write(home.join(".gemini/config/hooks.json"), "{}").unwrap();
    let stale = swarm(
        &home,
        &[
            "managed",
            "revert",
            "--all",
            "--digest",
            plan["digest"].as_str().unwrap(),
        ],
    );
    assert!(
        stderr(&stale)
            .contains("swarm: a managed file changed after the plan; check the plan again"),
        "{stale:?}"
    );
    let unknown = swarm(&home, &["managed", "revert", "000000000000"]);
    assert!(
        stderr(&unknown).contains("swarm: no managed entry 000000000000; run swarm managed list")
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// A guard group and its Codex trust key go together, the owner's own groups stay, and a Codex
/// group with an owner's group after it is a conflict, because Codex keys trust by place.
#[test]
fn a_guard_revert_takes_its_trust_key_and_keeps_the_owners_groups() {
    let home = scratch("revert-guard");
    write(&home, ".swarm/guards.json", RULES);
    let owner_group = r#"{"matcher": "Bash", "hooks": [{"type": "command", "command": "owner-guard", "timeout": 3}]}"#;
    let owner_settings = format!(r#"{{"hooks": {{"PreToolUse": [{owner_group}]}}}}"#);
    write(&home, ".claude/settings.json", &owner_settings);
    assert!(swarm(&home, &["setup", "hooks", "--yes"]).status.success());
    let entries = listing(&home);
    let codex_hooks = home.join(".codex/hooks.json");
    let group = entries
        .iter()
        .find(|entry| entry["file"] == codex_hooks.display().to_string().as_str())
        .unwrap();
    let id = group["id"].as_str().unwrap();

    // An owner's group after swarm's moves when swarm's goes, so Codex would ask again.
    let hooks = std::fs::read_to_string(&codex_hooks).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&hooks).unwrap();
    value["hooks"]["PreToolUse"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::from_str(owner_group).unwrap());
    std::fs::write(&codex_hooks, value.to_string()).unwrap();
    let blocked = swarm(&home, &["managed", "revert", id]);
    assert!(!blocked.status.success());
    assert!(
        stderr(&blocked).contains("move swarm's group last"),
        "{blocked:?}"
    );
    let order = swarm(&home, &["managed", "revert", id, "--plan", "--json"]);
    let order: serde_json::Value = serde_json::from_slice(&order.stdout).unwrap();
    assert_eq!(order["conflicts"][0]["kind"], "order", "{order}");
    std::fs::write(&codex_hooks, &hooks).unwrap();

    let revert = swarm(&home, &["managed", "revert", id]);
    assert!(revert.status.success(), "{revert:?}");
    let config = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    assert!(!config.contains("hooks.json:pre_tool_use"), "{config}");
    assert!(config.contains(":stop:1:0"), "{config}");
    assert_eq!(std::fs::read_to_string(&codex_hooks).unwrap(), "{}\n");

    assert!(
        swarm(&home, &["managed", "revert", "--all"])
            .status
            .success()
    );
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join(".claude/settings.json")).unwrap())
            .unwrap();
    let owner: serde_json::Value = serde_json::from_str(&owner_settings).unwrap();
    assert_eq!(settings, owner);
    assert_eq!(
        std::fs::read_to_string(home.join(".gemini/config/hooks.json")).unwrap(),
        "{}\n"
    );
    assert!(listing(&home).iter().all(|entry| entry["state"] == "off"));
    std::fs::remove_dir_all(&home).unwrap();
}

/// A child agent cannot revert the owner's managed changes, as it cannot launch agents: the
/// guard registration that blocks its own calls is one of them.
#[test]
fn a_child_agent_cannot_revert_managed_changes() {
    let home = scratch("revert-child");
    assert!(swarm(&home, &["setup", "hooks", "--yes"]).status.success());
    let codex = home.join(".codex/config.toml");
    let set_up = std::fs::read_to_string(&codex).unwrap();

    for (name, value) in [("SWARM_AGENT_ID", "coder"), ("HERDR_AGENT_PANE", "1")] {
        let revert = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &home)
            .env("SWARM_HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .env(name, value)
            .current_dir(&home)
            .args(["managed", "revert", "--all"])
            .output()
            .unwrap();
        assert!(!revert.status.success(), "{revert:?}");
        assert!(stderr(&revert).contains("child agent"), "{revert:?}");
    }
    assert_eq!(std::fs::read_to_string(&codex).unwrap(), set_up);
    std::fs::remove_dir_all(&home).unwrap();
}
