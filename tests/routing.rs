use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CONFIG: &str = r#"{
  "routes": {"code": ["codex-sol-high-agent", "claude-opus-high-agent"]},
  "runners": {
    "codex-sol-high-agent": {"provider": "codex", "model": "gpt-sol", "effort": "high"},
    "claude-opus-high-agent": {"provider": "claude", "model": "opus"}
  }
}"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-routing-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs the built binary with only HOME set, so no user config or routing env leaks in.
fn swarm(home: &Path, env: &[(&str, &Path)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command.env_clear().env("HOME", home).args(args);
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().unwrap()
}

#[test]
fn roles_get_reads_the_xdg_config() {
    let home = scratch("get");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(home.join(".config/agent-routing/roles.json"), CONFIG).unwrap();

    let output = swarm(&home, &[], &["roles", "get", "code", "--provider", "claude"]);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let resolved: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(resolved["runnerId"], "claude-opus-high-agent");
    assert_eq!(resolved["role"], "code");
}

#[test]
fn a_missing_explicit_config_is_an_error() {
    let home = scratch("missing");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(home.join(".config/agent-routing/roles.json"), CONFIG).unwrap();

    let output = swarm(&home, &[("AGENT_ROUTING_CONFIG", &home.join("nope.json"))], &["roles", "get", "code"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("AGENT_ROUTING_CONFIG names a missing file"));
}

#[test]
fn set_model_writes_through_the_symlink_and_keeps_a_backup() {
    let home = scratch("set");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(home.join("real.json"), CONFIG).unwrap();
    let link = home.join(".config/agent-routing/roles.json");
    std::os::unix::fs::symlink(home.join("real.json"), &link).unwrap();

    let output = swarm(&home, &[], &["roles", "set-model", "codex-sol-high-agent", "gpt-sol-2"]);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(home.join("real.json")).unwrap()).unwrap();
    assert_eq!(saved["runners"]["codex-sol-high-agent"]["model"], "gpt-sol-2");
    let backups = std::fs::read_dir(home.join(".config/agent-routing")).unwrap()
        .filter(|entry| entry.as_ref().unwrap().file_name().to_string_lossy().ends_with(".bak"))
        .count();
    assert_eq!(backups, 1);
}
