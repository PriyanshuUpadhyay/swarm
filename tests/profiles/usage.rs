// Tests and fixtures for provider account usage.
use super::*;

/// Native auth fixtures and the Claude usage cache.
fn claude_accounts(bin: &Path, claude_rows: &str) {
    use std::os::unix::fs::PermissionsExt;
    let home = bin.parent().unwrap();
    let profiles = home.join(".claude/.profiles");
    if profiles.exists() {
        std::fs::remove_dir_all(&profiles).unwrap();
    }
    std::fs::create_dir_all(&profiles).unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(claude_rows).unwrap();
    let mut auth = String::from("#!/bin/sh\ncase \"$CLAUDE_CONFIG_DIR\" in\n");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut meters = Vec::new();
    for row in rows {
        let name = row["name"].as_str().unwrap();
        std::fs::create_dir_all(profiles.join(name)).unwrap();
        auth.push_str(&format!(
            "  */{name}) echo '{{\"loggedIn\":{}}}' ;;\n",
            row["signed_in"]
        ));
        meters.push(serde_json::json!({"home":std::fs::canonicalize(profiles.join(name)).unwrap(),"meters":[{"provider":"claude","account":name,"label":name,"window":"5h","window_minutes":300,"used_pct":100-row["remaining"].as_i64().unwrap(),"reset_time_seconds":null,"state":"fresh","source":"claude_api","reason":null,"as_of_seconds":now}]}));
    }
    auth.push_str("  *) echo '{\"loggedIn\":false}' ;;\nesac\n");
    std::fs::write(bin.join("claude"), auth).unwrap();
    json(&swarm(
        home,
        &[],
        &["accounts", "--provider", "agy", "--json"],
    ));
    std::fs::write(
        home.join(".swarm/claude-usage.json"),
        serde_json::json!({"accounts":meters}).to_string(),
    )
    .unwrap();
    std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o755)).unwrap();
}

const CLAUDE_FIRST: &str = r#"{
  "routes": {"code": ["claude-opus-high-agent", "codex-sol-high-agent"]},
  "runners": {
    "claude-opus-high-agent": {"provider": "claude", "model": "opus", "effort": "high"},
    "codex-sol-high-agent": {"provider": "codex", "model": "gpt-sol", "effort": "high"}
  }
}"#;

#[test]
fn a_provider_with_every_account_low_or_signed_out_is_skipped() {
    let home = scratch("usage");
    old_config(&home, CLAUDE_FIRST);
    let bin = clis(&home, &["claude", "codex"]);
    let path = [("PATH", bin.as_path())];

    claude_accounts(
        &bin,
        r#"[{"name":"nearly-spent","dir":"/p/nearly-spent","signed_in":true,"remaining":2},{"name":"signed-out","dir":"/p/signed-out","signed_in":false,"remaining":90}]"#,
    );
    let low = swarm(&home, &path, &["roles", "get", "code"]);
    let resolved = json(&low);
    assert_eq!(resolved["runnerId"], "code#2");
    assert_eq!(resolved["skipped"][0]["code"], "low_usage");
    assert!(
        String::from_utf8_lossy(&low.stderr)
            .contains("swarm: code: skipped claude/opus/high: usage 2% left (threshold 5%)")
    );

    claude_accounts(
        &bin,
        r#"[{"name":"signed-out","dir":"/p/signed-out","signed_in":false,"remaining":80}]"#,
    );
    let signed_out = json(&swarm(&home, &path, &["roles", "get", "code"]));
    assert_eq!(signed_out["runnerId"], "code#2");
    assert_eq!(signed_out["skipped"][0]["code"], "signed_out");
    assert_eq!(
        signed_out["skipped"][0]["text"],
        "no claude account is signed in"
    );

    claude_accounts(
        &bin,
        r#"[{"name":"personal","dir":"/p/personal","signed_in":true,"remaining":40}]"#,
    );
    let healthy = json(&swarm(&home, &path, &["roles", "get", "code"]));
    assert_eq!(healthy["runnerId"], "code#1");

    let check = json(&swarm(&home, &path, &["roles", "check", "--json"]));
    let code = check["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["name"] == "code")
        .unwrap();
    assert_eq!(code["pick"], 0);
    assert_eq!(code["skipped"], serde_json::json!([]));
}

#[test]
fn a_usage_read_that_hangs_counts_as_can_run_after_its_deadline() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("slow");
    old_config(&home, CLAUDE_FIRST);
    let bin = clis(&home, &["claude", "codex"]);
    // A full path: a copy of /bin/sleep in the test's bin dir is killed by code signing.
    std::fs::write(bin.join("claude"), "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o755)).unwrap();

    let started = std::time::Instant::now();
    let resolved = json(&swarm(&home, &[("PATH", &bin)], &["roles", "get", "code"]));

    assert_eq!(resolved["runnerId"], "code#1");
    let waited = started.elapsed();
    assert!(waited >= std::time::Duration::from_secs(2), "{waited:?}");
    assert!(waited < std::time::Duration::from_secs(5), "{waited:?}");
}
