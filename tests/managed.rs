use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-managed-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// The built binary with only HOME, PATH, and SWARM_HOME set, so no provider config of this Mac
/// is read and SWARM_HOME pins the data to HOME for a branch build too (ADR 0027).
fn swarm(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(home)
        .args(args)
        .output()
        .unwrap()
}

/// The six Codex state-hook trust keys that `hooks setup` adds, as paths in `config.toml`.
fn codex_state_paths() -> Vec<serde_json::Value> {
    [
        "user_prompt_submit",
        "pre_tool_use",
        "post_tool_use",
        "permission_request",
        "stop",
        "interrupt",
    ]
    .map(|label| {
        serde_json::json!([
            "hooks",
            "state",
            format!("/<session-flags>/config.toml:{label}:1:0"),
            "trusted_hash"
        ])
    })
    .to_vec()
}

/// On a fresh HOME, `hooks setup` records each item it adds: six Codex trust keys and AGY's
/// `swarm` group (ADR 0042).
#[test]
fn hooks_setup_records_each_item_it_adds() {
    let home = scratch("record");
    let setup = swarm(&home, &["hooks", "setup"]);
    assert!(setup.status.success(), "{setup:?}");

    let db = rusqlite::Connection::open(home.join(".swarm/swarm.db")).unwrap();
    let mut rows: Vec<(String, String, String, serde_json::Value)> = db
        .prepare("SELECT writer, kind, file, path FROM managed_edit WHERE off = 0")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                serde_json::from_str(&row.get::<_, String>(3)?).unwrap(),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.sort_by_key(|row| row.3.to_string());
    let codex = home.join(".codex/config.toml").display().to_string();
    let agy = home.join(".gemini/config/hooks.json").display().to_string();
    let mut wanted: Vec<_> = codex_state_paths()
        .into_iter()
        .map(|path| ("hooks.state".into(), "toml_key".into(), codex.clone(), path))
        .collect();
    wanted.push((
        "hooks.state".into(),
        "json_key".into(),
        agy,
        serde_json::json!(["swarm"]),
    ));
    wanted.sort_by_key(|row| row.3.to_string());
    assert_eq!(rows, wanted);
    std::fs::remove_dir_all(&home).unwrap();
}

fn write(home: &Path, file: &str, text: &str) {
    let path = home.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn listing(home: &Path) -> Vec<serde_json::Value> {
    let list = swarm(home, &["managed", "list", "--json"]);
    assert!(list.status.success(), "{list:?}");
    let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    list["entries"].as_array().unwrap().clone()
}

/// The guard rule list that makes `hooks setup` register the guard too (ADR 0040).
const RULES: &str =
    r#"{"rules": [{"name": "no", "event": "PreToolUse", "command": ["/bin/true"]}]}"#;

/// On a fresh HOME with no provider config, `managed list --json` shows every item that hooks
/// setup wrote, each present and recorded. An item someone else changed shows what is there now,
/// a removed one is gone, and items that equal swarm's text with no row are found, not recorded.
#[test]
fn managed_list_shows_each_hooks_setup_write_and_its_live_state() {
    let home = scratch("list");
    write(&home, ".swarm/guards.json", RULES);
    assert!(swarm(&home, &["hooks", "setup"]).status.success());

    let entries = listing(&home);
    let codex = home.join(".codex/config.toml").display().to_string();
    let codex_hooks = home.join(".codex/hooks.json").display().to_string();
    let claude = home.join(".claude/settings.json").display().to_string();
    let agy = home.join(".gemini/config/hooks.json").display().to_string();
    let mut places: Vec<(String, String, String)> = entries
        .iter()
        .map(|entry| {
            assert_eq!(entry["state"], "present", "{entry}");
            assert_eq!(entry["recorded"], true, "{entry}");
            assert!(
                entry["found"].is_null() && entry["before"].is_null(),
                "{entry}"
            );
            assert!(entry["at_s"].as_i64().unwrap() > 0, "{entry}");
            (
                entry["writer"].as_str().unwrap().into(),
                entry["kind"].as_str().unwrap().into(),
                entry["file"].as_str().unwrap().into(),
            )
        })
        .collect();
    places.sort();
    let mut wanted = vec![
        (
            "hooks.state".to_string(),
            "toml_key".to_string(),
            codex.clone()
        );
        6
    ];
    wanted.extend([
        ("hooks.guard".into(), "toml_key".into(), codex.clone()),
        (
            "hooks.guard".into(),
            "json_array_item".into(),
            codex_hooks.clone(),
        ),
        (
            "hooks.guard".into(),
            "json_array_item".into(),
            claude.clone(),
        ),
        ("hooks.state".into(), "json_key".into(), agy.clone()),
        ("hooks.guard".into(), "json_key".into(), agy.clone()),
    ]);
    wanted.sort();
    assert_eq!(places, wanted);
    // The Codex guard trust key goes with the hooks.json group it trusts.
    let group = entries
        .iter()
        .find(|entry| entry["file"] == codex_hooks.as_str())
        .unwrap();
    let trust = entries
        .iter()
        .find(|entry| entry["writer"] == "hooks.guard" && entry["file"] == codex.as_str())
        .unwrap();
    assert_eq!(trust["with"], group["id"]);

    // The owner edits one trust hash and deletes AGY's file.
    let config = std::fs::read_to_string(&codex).unwrap();
    let stop = entries
        .iter()
        .find(|entry| {
            entry["path"][2]
                .as_str()
                .is_some_and(|key| key.ends_with(":stop:1:0"))
        })
        .unwrap();
    let hash = stop["wrote"].as_str().unwrap();
    std::fs::write(&codex, config.replace(hash, "sha256:edited")).unwrap();
    std::fs::remove_file(&agy).unwrap();
    let state = |entries: &[serde_json::Value], id: &serde_json::Value| {
        let entry = entries
            .iter()
            .find(|entry| entry["id"] == *id)
            .unwrap()
            .clone();
        (entry["state"].clone(), entry["found"].clone())
    };
    let entries = listing(&home);
    assert_eq!(
        state(&entries, &stop["id"]),
        ("changed".into(), "sha256:edited".into())
    );
    let agy_ids: Vec<_> = entries
        .iter()
        .filter(|entry| entry["file"] == agy.as_str())
        .collect();
    assert_eq!(agy_ids.len(), 2);
    assert!(agy_ids.iter().all(|entry| entry["state"] == "gone"));

    // With no rows, each item that still equals swarm's text is found, not recorded.
    let db = rusqlite::Connection::open(home.join(".swarm/swarm.db")).unwrap();
    db.execute("DELETE FROM managed_edit", []).unwrap();
    let found = listing(&home);
    assert_eq!(found.len(), 8, "{found:?}");
    assert!(found.iter().all(|entry| entry["recorded"] == false
        && entry["state"] == "present"
        && entry["at_s"].is_null()));
    assert!(found.iter().all(|entry| entry["id"] != stop["id"]));

    let claude_group = found
        .iter()
        .find(|entry| entry["file"] == claude.as_str())
        .unwrap();
    let text = swarm(&home, &["managed", "list"]);
    let text = String::from_utf8_lossy(&text.stdout).into_owned();
    assert_eq!(text.lines().count(), 8, "{text}");
    assert!(
        text.contains(&format!(
            "{}  present  hooks.guard  {claude}  hooks.PreToolUse\n",
            claude_group["id"].as_str().unwrap()
        )),
        "{text}"
    );
    assert!(text.contains(&format!("  present  hooks.state  {codex}  hooks.state./<session-flags>/config.toml:interrupt:1:0.trusted_hash\n")), "{text}");

    // A found item can be removed too; the owner's edited key is not swarm's, so it stays.
    let revert = swarm(&home, &["managed", "revert", "--all"]);
    assert!(revert.status.success(), "{revert:?}");
    assert!(listing(&home).is_empty());
    let config = std::fs::read_to_string(&codex).unwrap();
    assert_eq!(config.matches("trusted_hash").count(), 1, "{config}");
    assert!(config.contains("sha256:edited"), "{config}");
    std::fs::remove_dir_all(&home).unwrap();
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The id of the entry whose path ends in Codex's `stop` state-hook key.
fn stop_entry(entries: &[serde_json::Value]) -> serde_json::Value {
    entries
        .iter()
        .find(|entry| {
            entry["path"][2]
                .as_str()
                .is_some_and(|key| key.ends_with(":stop:1:0"))
        })
        .unwrap()
        .clone()
}

/// Done-when 1: a revert of one Codex trust key removes only that key, and the owner's own key in
/// the same `hooks.state` table stays byte for byte. Revert of all gives the owner's file back.
#[test]
fn a_revert_removes_only_swarms_key_and_keeps_the_owners_key_in_the_same_table() {
    let home = scratch("revert-one");
    let owners = "model = \"o3\"\n\n[hooks.state.\"/owner/config.toml:stop:0:0\"]\ntrusted_hash = \"sha256:owner\"\n";
    write(&home, ".codex/config.toml", owners);
    assert!(swarm(&home, &["hooks", "setup"]).status.success());
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
    assert!(swarm(&home, &["hooks", "setup"]).status.success());
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
    assert!(swarm(&home, &["hooks", "setup"]).status.success());
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
