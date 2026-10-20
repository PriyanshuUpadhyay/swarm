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

/// The six Codex state-hook trust keys that `setup hooks` adds, as paths in `config.toml`.
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

/// On a fresh HOME, `setup hooks` records each item it adds: six Codex trust keys and AGY's
/// `swarm` group (ADR 0042).
#[test]
fn hooks_setup_records_each_item_it_adds() {
    let home = scratch("record");
    let setup = swarm(&home, &["setup", "hooks", "--yes"]);
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

/// The guard rule list that makes `setup hooks` register the guard too (ADR 0040).
const RULES: &str =
    r#"{"rules": [{"name": "no", "event": "PreToolUse", "command": ["/bin/true"]}]}"#;

/// On a fresh HOME with no provider config, `managed list --json` shows every item that hooks
/// setup wrote, each present and recorded. An item someone else changed shows what is there now,
/// a removed one is gone, and items that equal swarm's text with no row are found, not recorded.
#[test]
fn managed_list_shows_each_hooks_setup_write_and_its_live_state() {
    let home = scratch("list");
    write(&home, ".swarm/guards.json", RULES);
    assert!(swarm(&home, &["setup", "hooks", "--yes"]).status.success());

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

    // A found item can be removed too; the owner's edited key is not swarm's, so it stays. Each
    // removed item keeps a row that shows it off, so the app can offer On.
    let revert = swarm(&home, &["managed", "revert", "--all"]);
    assert!(revert.status.success(), "{revert:?}");
    let ids = |entries: &[serde_json::Value]| {
        let mut ids: Vec<_> = entries.iter().map(|entry| entry["id"].clone()).collect();
        ids.sort_by_key(|id| id.to_string());
        ids
    };
    let reverted = listing(&home);
    assert_eq!(ids(&reverted), ids(&found));
    assert!(
        reverted
            .iter()
            .all(|entry| entry["recorded"] == true && entry["state"] == "off"),
        "{reverted:?}"
    );
    let config = std::fs::read_to_string(&codex).unwrap();
    assert_eq!(config.matches("trusted_hash").count(), 1, "{config}");
    assert!(config.contains("sha256:edited"), "{config}");

    // On runs the owning writer's plan again: once the owner sets the edited key back,
    // `setup hooks` writes each off item and its row shows it present.
    std::fs::write(&codex, config.replace("sha256:edited", hash)).unwrap();
    let setup = swarm(&home, &["setup", "hooks", "--yes"]);
    assert!(setup.status.success(), "{setup:?}");
    let on = listing(&home);
    assert!(
        found.iter().all(|item| on
            .iter()
            .any(|entry| entry["id"] == item["id"] && entry["state"] == "present")),
        "{on:?}"
    );
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

/// Under a linked `~/.gemini` with no `config` folder yet, the AGY item swarm writes and the item
/// that `list` finds there are one place, so it has one id and `revert --all` plans the file once.
#[test]
fn an_item_under_a_linked_folder_made_by_the_write_has_one_id() {
    let home = scratch("linked-parent");
    std::fs::create_dir_all(home.join("dot/gemini")).unwrap();
    std::os::unix::fs::symlink(home.join("dot/gemini"), home.join(".gemini")).unwrap();
    assert!(swarm(&home, &["setup", "hooks", "--yes"]).status.success());

    let entries = listing(&home);
    let agy: Vec<_> = entries
        .iter()
        .filter(|entry| entry["path"] == serde_json::json!(["swarm"]))
        .collect();
    assert_eq!(agy.len(), 1, "{entries:?}");
    assert_eq!(agy[0]["recorded"], true);
    assert_eq!(
        agy[0]["file"],
        home.join("dot/gemini/config/hooks.json")
            .display()
            .to_string()
    );
    let plan = swarm(&home, &["managed", "revert", "--all", "--plan"]);
    let plan = String::from_utf8_lossy(&plan.stdout).into_owned();
    assert_eq!(plan.matches("+++ ").count(), 2, "{plan}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// A setup that fails at a later file names each file it already wrote, so the owner knows what
/// changed; `managed list` shows those items too.
#[test]
fn a_partial_hooks_setup_names_the_files_it_already_wrote() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("partial");
    // The write makes AGY's missing config folder, which a read-only parent refuses.
    let agy_dir = home.join(".gemini");
    std::fs::create_dir_all(&agy_dir).unwrap();
    std::fs::set_permissions(&agy_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let setup = swarm(&home, &["setup", "hooks", "--yes"]);
    std::fs::set_permissions(&agy_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!setup.status.success(), "{setup:?}");
    let error = stderr(&setup);
    assert!(error.contains("cannot write"), "{error}");
    let codex = home.join(".codex/config.toml");
    let out = String::from_utf8_lossy(&setup.stdout);
    assert!(
        out.contains(&format!("swarm: hooks: wrote {}", codex.display()))
            && error.contains("each file named above is written and recorded"),
        "{out}\n{error}"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// A Claude home picked with `CLAUDE_CONFIG_DIR` is a guard target too, through the same plan and
/// record as `~/.claude/settings.json`.
#[test]
fn hooks_setup_guards_the_claude_home_that_claude_config_dir_names() {
    let home = scratch("claude-config-dir");
    write(&home, ".swarm/guards.json", RULES);
    let work = home.join("claude-work");
    std::fs::create_dir_all(&work).unwrap();
    let setup = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .env("CLAUDE_CONFIG_DIR", &work)
        .current_dir(&home)
        .args(["setup", "hooks", "--yes"])
        .output()
        .unwrap();
    assert!(setup.status.success(), "{setup:?}");
    let db = rusqlite::Connection::open(home.join(".swarm/swarm.db")).unwrap();
    let files: Vec<String> = db
        .prepare("SELECT file FROM managed_edit WHERE off = 0 AND writer = 'hooks.guard'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for settings in [
        home.join(".claude/settings.json"),
        work.join("settings.json"),
    ] {
        let settings = settings.display().to_string();
        assert!(files.contains(&settings), "{settings} not in {files:?}");
    }
    // An empty variable names no home, so no relative `settings.json` becomes a target.
    let empty = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .env("CLAUDE_CONFIG_DIR", "")
        .current_dir(&home)
        .args(["setup", "hooks", "--plan", "--json"])
        .output()
        .unwrap();
    assert!(empty.status.success(), "{empty:?}");
    assert!(
        !String::from_utf8_lossy(&empty.stdout).contains("\"settings.json\""),
        "{}",
        String::from_utf8_lossy(&empty.stdout)
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[path = "managed/revert.rs"]
mod revert;
