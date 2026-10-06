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
    std::fs::remove_dir_all(&home).unwrap();
}
