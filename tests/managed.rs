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
