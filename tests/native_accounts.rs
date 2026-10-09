use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(role: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("swarm-native-{role}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::canonicalize(home).unwrap()
}

fn tool(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn accounts(home: &Path, provider: &str, extra: &[(&str, &Path)]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .envs(extra.iter().copied())
        .args(["accounts", "--provider", provider, "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn native_claude_discovery_keeps_default_and_deduplicates_active_home() {
    let home = fixture("work");
    let work = home.join(".claude/.profiles/work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(home.join(".claude/.profiles/personal")).unwrap();
    std::os::unix::fs::symlink(&work, home.join(".claude/.profiles/spare")).unwrap();
    tool(
        &home,
        "claude",
        r#"test "$*" = 'auth status' || exit 9
case "$CLAUDE_CONFIG_DIR" in
  */personal) echo '{"loggedIn":false}'; exit 1 ;;
  *) echo '{"loggedIn":true,"email":"owner@example.test"}' ;;
esac"#,
    );
    let list = accounts(&home, "claude", &[("CLAUDE_CONFIG_DIR", &work)]);
    assert_eq!(list["source"], "swarm");
    assert_eq!(list["state"], "ready");
    assert_eq!(list["auto"], "work");
    let rows = list["accounts"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["name"], "default");
    assert_eq!(rows[0]["env"], serde_json::json!({}));
    assert_eq!(rows[1]["auth_state"], "signed_out");
    assert_eq!(rows[2]["name"], "work");
    assert_eq!(rows[2]["env"]["AGENT_PROFILE_LABEL"], "work");
    assert_eq!(rows[2]["usage_state"], "failed");
    assert_eq!(rows[2]["remaining_pct"], serde_json::Value::Null);
}

#[test]
fn an_unavailable_auth_read_is_not_signed_out() {
    let home = fixture("personal");
    tool(
        &home,
        "claude",
        "echo 'secret must not reach the result' >&2; exit 1",
    );
    let list = accounts(&home, "claude", &[]);
    assert_eq!(list["state"], "ready");
    assert_eq!(list["accounts"][0]["auth_state"], "unavailable");
    assert_eq!(list["auto"], serde_json::Value::Null);
    assert!(!list.to_string().contains("secret"));
}

#[test]
fn agy_has_the_contract_no_source_shape() {
    let home = fixture("spare");
    let mut list = accounts(&home, "agy", &[]);
    list["revision"] = "opaque".into();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/accounts/spare-no-source.json")).unwrap();
    assert_eq!(list, expected);
}

#[test]
fn codex_identity_uses_the_protocol_and_active_named_home_without_yelo() {
    let home = fixture("codex-work");
    let work = home.join(".codex-work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(home.join(".codex-personal")).unwrap();
    tool(
        &home,
        "codex",
        include_str!("fixtures/accounts/work-app-server.sh"),
    );
    let list = accounts(&home, "codex", &[("CODEX_HOME", &work)]);
    assert_eq!(list["state"], "ready");
    assert_eq!(list["auto"], "work");
    assert_eq!(list["accounts"][0]["name"], "default");
    assert_eq!(list["accounts"][1]["auth_state"], "signed_out");
    assert_eq!(list["accounts"][2]["email"], "owner@example.test");
    let unchanged = accounts(&home, "codex", &[]);
    assert_eq!(unchanged["auto"], "default");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cache = serde_json::json!({"accounts":[{"home":work,"meters":[
        {"provider":"codex","account":"work","label":"work","used_pct":10,"state":"fresh","as_of_seconds":now},
        {"provider":"codex","account":"work","label":"work","used_pct":30,"state":"fresh","as_of_seconds":now}
    ]}]});
    std::fs::write(home.join(".swarm/codex-usage.json"), cache.to_string()).unwrap();
    let cached = accounts(&home, "codex", &[]);
    assert_eq!(cached["auto"], "work");
    assert_eq!(cached["accounts"][2]["remaining_pct"], 70);
    std::os::unix::fs::symlink(&work, home.join(".codex")).unwrap();
    let deduplicated = accounts(&home, "codex", &[("CODEX_HOME", &work)]);
    assert_eq!(deduplicated["accounts"].as_array().unwrap().len(), 2);
    assert_eq!(deduplicated["accounts"][0]["name"], "default");
    assert_eq!(deduplicated["auto"], "default");
}

#[test]
fn cached_windows_choose_least_left_and_match_the_shared_account_fixture() {
    use swarm::profiles::{Account, AuthState, apply_usage, empty_accounts, pick_auto};
    let mut list = empty_accounts("codex");
    list.state = "ready".into();
    list.source = Some("swarm".into());
    list.revision = "opaque".into();
    list.accounts.push(Account {
        name: "work".into(),
        email: Some("owner@example.test".into()),
        home: "/tmp/demo/.codex-work".into(),
        env: std::collections::BTreeMap::from([(
            "CODEX_HOME".into(),
            "/tmp/demo/.codex-work".into(),
        )]),
        auth_state: AuthState::SignedIn,
        remaining_pct: None,
        usage_state: "missing".into(),
        usage_source: Some("codex_app_server".into()),
        summary: None,
    });
    let mut meters: Vec<swarm::profiles::UsageMeter> = serde_json::from_value(serde_json::json!([
        {"provider":"codex","account":"work","label":"work","used_pct":10,"state":"fresh","as_of_seconds":700},
        {"provider":"codex","account":"work","label":"work","used_pct":30,"state":"fresh","as_of_seconds":700}
    ])).unwrap();
    apply_usage(&mut list, &meters);
    list.auto = pick_auto(&list.accounts, None);
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/accounts/work-list.json")).unwrap();
    assert_eq!(serde_json::to_value(&list).unwrap(), expected);
    for meter in &mut meters {
        meter.state = "stale".into();
    }
    apply_usage(&mut list, &meters);
    assert_eq!(list.accounts[0].usage_state, "stale");
    assert_eq!(pick_auto(&list.accounts, None), None);
    assert_eq!(
        pick_auto(&list.accounts, Some("/tmp/demo/.codex-work")).as_deref(),
        Some("work")
    );
    meters[0].state = "failed".into();
    meters[0].used_pct = None;
    apply_usage(&mut list, &meters);
    assert_eq!(list.accounts[0].usage_state, "failed");
    assert_eq!(list.accounts[0].remaining_pct, None);
}

#[test]
fn an_unavailable_row_keeps_other_native_accounts_visible() {
    let home = fixture("spare-partial-auth");
    std::fs::create_dir_all(home.join(".claude/.profiles/work")).unwrap();
    tool(
        &home,
        "claude",
        r#"test "$*" = 'auth status' || exit 9
case "$CLAUDE_CONFIG_DIR" in
  */work) echo '{"loggedIn":true}' ;;
  *) echo '{"loggedIn":false}'; exit 2 ;;
esac"#,
    );
    let list = accounts(&home, "claude", &[]);
    assert_eq!(list["state"], "ready");
    assert_eq!(list["accounts"][0]["auth_state"], "unavailable");
    assert_eq!(list["accounts"][1]["auth_state"], "signed_in");
    std::fs::remove_file(home.join("bin/claude")).unwrap();
    let unavailable = accounts(&home, "claude", &[]);
    assert_eq!(unavailable["state"], "unavailable");
}

#[test]
fn missing_cli_has_a_typed_native_read_error() {
    use swarm::profiles::native::{NativeReadError, read_json};
    let error = read_json(
        "/missing/provider-work",
        &["auth", "status"],
        &std::collections::BTreeMap::new(),
        swarm::profiles::native::deadline(2),
    )
    .unwrap_err();
    assert!(matches!(error, NativeReadError::CliUnavailable));
    assert_eq!(error.to_string(), "provider CLI is unavailable");
}

#[test]
fn reserved_discovered_names_are_skipped_so_external_current_keeps_its_name() {
    let home = fixture("spare-reserved-discovery");
    for name in ["current", "auto", "default"] {
        std::fs::create_dir_all(home.join(format!(".codex-{name}"))).unwrap();
    }
    let external = home.join("external-work");
    std::fs::create_dir(&external).unwrap();
    tool(
        &home,
        "codex",
        include_str!("fixtures/accounts/work-app-server.sh"),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("CODEX_HOME", &external)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .args(["accounts", "--provider", "codex", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = list["accounts"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let current = rows.iter().find(|row| row["name"] == "current").unwrap();
    assert_eq!(current["home"], external.to_string_lossy().as_ref());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid account name"));
}
