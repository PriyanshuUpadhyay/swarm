use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(role: &str) -> PathBuf {
    let home =
        std::env::temp_dir().join(format!("swarm-native-usage-{role}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::create_dir_all(home.join(".codex-work")).unwrap();
    std::fs::create_dir_all(home.join(".codex-personal")).unwrap();
    let home = std::fs::canonicalize(home).unwrap();
    tool(
        &home,
        "codex",
        include_str!("fixtures/accounts/work-usage-app-server.sh"),
    );
    tool(
        &home,
        "claude",
        "echo '{\"loggedIn\":true,\"email\":\"owner@example.test\"}'",
    );
    std::fs::write(home.join("limits.json"), r#"{"id":3,"result":{"rateLimits":{"primary":{"usedPercent":30,"windowDurationMins":300,"resetsAt":1791540000},"secondary":null}}}"#).unwrap();
    home
}

fn tool(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .args(args)
        .output()
        .unwrap()
}

fn json(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn refresh_reads_native_windows_and_caches_only_nonsecret_usage() {
    let home = fixture("work");
    let usage = json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let rows = usage["meters"].as_array().unwrap();
    let mut work = rows
        .iter()
        .find(|row| row["account"] == "work")
        .unwrap()
        .clone();
    assert_eq!(work["state"], "fresh");
    assert_eq!(work["window_minutes"], 300);
    assert_eq!(work["used_pct"], 30);
    work["as_of_seconds"] = 1791530000i64.into();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/accounts/work-usage.json")).unwrap();
    assert_eq!(serde_json::json!({"meters":[work]}), expected);
    let personal = rows
        .iter()
        .find(|row| row["account"] == "personal")
        .unwrap();
    assert_eq!(personal["state"], "no_source");
    assert!(personal["used_pct"].is_null());
    assert!(personal["source"].is_null());
    let cache = std::fs::read_to_string(home.join(".swarm/codex-usage.json")).unwrap();
    assert!(!cache.contains("token"));
    assert!(!cache.contains("owner@example.test"));
    assert!(!home.join(".codex-work/codex-usage.json").exists());
    let before = std::fs::read_to_string(home.join("requests")).unwrap();
    assert_eq!(
        before
            .lines()
            .filter(|line| line.contains("account/rateLimits/read"))
            .count(),
        1
    );
    let cached = json(run(&home, &["usage", "--json"]));
    assert!(
        cached["meters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["account"] == "work" && row["state"] == "fresh")
    );
    let after = std::fs::read_to_string(home.join("requests")).unwrap();
    assert_eq!(
        after
            .lines()
            .filter(|line| line.contains("account/rateLimits/read"))
            .count(),
        1
    );
}

#[test]
fn distinct_limit_buckets_and_windows_remain_distinct() {
    let home = fixture("personal");
    std::fs::write(home.join("limits.json"), r#"{"id":3,"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":10,"windowDurationMins":15,"resetsAt":1791540000}},"rateLimitsByLimitId":{"codex":{"limitId":"codex","primary":{"usedPercent":10,"windowDurationMins":15,"resetsAt":1791540000},"secondary":{"usedPercent":40,"windowDurationMins":10080,"resetsAt":1791541000}},"review":{"limitId":"review","primary":{"usedPercent":80,"windowDurationMins":60,"resetsAt":1791542000},"secondary":null}}}}"#).unwrap();
    let usage = json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let work: Vec<_> = usage["meters"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["account"] == "work")
        .collect();
    assert_eq!(work.len(), 3);
    assert_eq!(work[0]["window"], "codex:primary");
    assert_eq!(work[0]["window_minutes"], 15);
    assert_eq!(work[1]["window"], "codex:secondary");
    assert_eq!(work[2]["window"], "review:primary");
    let accounts = json(run(&home, &["accounts", "--provider", "codex", "--json"]));
    let work = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "work")
        .unwrap();
    assert_eq!(work["remaining_pct"], 20);
}

#[test]
fn invalid_claude_home_has_no_usage_with_matching_missing_or_failed_yelo() {
    let home = fixture("personal-invalid-usage");
    std::fs::create_dir_all(home.join(".claude/.profiles/Personal")).unwrap();
    for snapshot in [
        r#"echo '[{"provider":"claude","label":"cl·Personal","window":"5h","pct":30,"state":"ok"}]'"#,
        "echo '[]'",
        "exit 1",
    ] {
        tool(&home, "yelo", snapshot);
        let list = json(run(&home, &["accounts", "--provider", "claude", "--json"]));
        let account = list["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "Personal")
            .unwrap();
        let usage = json(run(&home, &["usage", "--json"]));
        let claude: Vec<_> = usage["meters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["provider"] == "claude")
            .collect();
        let invalid: Vec<_> = claude
            .iter()
            .filter(|row| row["account"] == "Personal")
            .collect();
        assert_eq!(invalid.len(), 1, "{snapshot}");
        assert_eq!(invalid[0]["state"], "no_source", "{snapshot}");
        assert_eq!(invalid[0]["reason"], account["summary"], "{snapshot}");
        assert!(
            invalid[0]["reason"]
                .as_str()
                .unwrap()
                .contains("lowercase ASCII")
        );
        assert!(invalid[0]["source"].is_null());
        assert!(
            claude.iter().all(|row| row["used_pct"].is_null()),
            "{snapshot}"
        );
    }
}

fn assert_invalid_alias_keeps_work_usage(home: &Path, alias: &str) {
    json(run(
        home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let list = json(run(home, &["accounts", "--provider", "codex", "--json"]));
    let accounts = list["accounts"].as_array().unwrap();
    let work = accounts.iter().find(|row| row["name"] == "work").unwrap();
    assert_eq!(work["usage_state"], "fresh");
    assert_eq!(work["remaining_pct"], 70);
    assert!(accounts.iter().all(|row| row["name"] != alias));
    let usage = json(run(home, &["usage", "--json"]));
    let work_meter = usage["meters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["provider"] == "codex" && row["account"] == "work")
        .unwrap();
    assert_eq!(work_meter["state"], "fresh");
    assert_eq!(work_meter["used_pct"], 30);
    let cache: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join(".swarm/codex-usage.json")).unwrap())
            .unwrap();
    assert_eq!(
        cache["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["home"] == work["home"])
            .count(),
        1
    );
}

#[test]
fn invalid_symlinked_codex_home_does_not_replace_work_cached_usage() {
    let home = fixture("work-invalid-symlink");
    std::os::unix::fs::symlink(home.join(".codex-work"), home.join(".codex-Spare")).unwrap();
    assert_invalid_alias_keeps_work_usage(&home, "Spare");
}

#[test]
fn invalid_case_alias_of_registered_codex_home_keeps_work_cached_usage() {
    let home = fixture("work-invalid-case-alias");
    let work = home.join(".codex-work");
    std::fs::rename(&work, home.join(".codex-Work")).unwrap();
    if !work.exists() {
        eprintln!("Skipped case-alias fixture because the temp volume is case-sensitive");
        return;
    }
    tool(
        &home,
        "codex",
        &include_str!("fixtures/accounts/work-usage-app-server.sh")
            .replace("*/.codex-work)", "*/.codex-work|*/.codex-Work)"),
    );
    json(run(&home, &["accounts", "--provider", "codex", "--json"]));
    std::fs::write(
        home.join(".swarm/accounts.toml"),
        format!(
            "version = 1\n[[accounts]]\nprovider = \"codex\"\nname = \"work\"\nhome = \"{}\"\n",
            work.display()
        ),
    )
    .unwrap();
    assert_invalid_alias_keeps_work_usage(&home, "Work");
}

#[test]
fn cached_claude_usage_uses_yelo_and_missing_sources_never_become_zero() {
    let home = fixture("spare");
    tool(
        &home,
        "yelo",
        r#"test "$*" = 'usage show --json' || exit 8
echo '[{"provider":"claude","label":"cl·owner@example.test","window":"7d","pct":12,"state":"ok","asOf":1},{"provider":"codex","label":"cx","pct":100,"state":"ok","asOf":1}]'"#,
    );
    let usage = json(run(&home, &["usage", "--json"]));
    let rows = usage["meters"].as_array().unwrap();
    let claude = rows.iter().find(|row| row["provider"] == "claude").unwrap();
    assert_eq!(claude["account"], "default");
    assert_eq!(claude["source"], "yelo");
    assert_eq!(claude["state"], "stale");
    assert_eq!(claude["used_pct"], 12);
    assert_eq!(claude["window_minutes"], 10080);
    let agy = rows.iter().find(|row| row["provider"] == "agy").unwrap();
    assert_eq!(agy["state"], "no_source");
    assert!(agy["used_pct"].is_null());
    assert!(
        rows.iter()
            .filter(|row| row["provider"] == "codex")
            .all(|row| row["used_pct"].is_null())
    );
}

#[test]
fn stale_missing_and_invalid_cache_have_distinct_states() {
    let home = fixture("work-cache");
    json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let path = home.join(".swarm/codex-usage.json");
    let mut cache: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let work = cache["accounts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["home"].as_str().unwrap().ends_with("/.codex-work"))
        .unwrap();
    work["meters"][0]["as_of_seconds"] = 0.into();
    std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
    let usage = json(run(&home, &["usage", "--json"]));
    let work = usage["meters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["account"] == "work")
        .unwrap();
    assert_eq!(work["state"], "stale");
    assert_eq!(work["used_pct"], 30);
    std::fs::write(&path, b"{invalid secret diagnostics").unwrap();
    let output = run(&home, &["usage", "--json"]);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret diagnostics"));
    let usage = json(output);
    let work = usage["meters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["account"] == "work")
        .unwrap();
    assert_eq!(work["state"], "failed");
    assert!(work["used_pct"].is_null());
    std::fs::remove_file(&path).unwrap();
    let usage = json(run(&home, &["usage", "--json"]));
    let work = usage["meters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["account"] == "work")
        .unwrap();
    assert_eq!(work["state"], "missing");
    assert!(work["used_pct"].is_null());
}

#[test]
fn invalid_percentages_rpc_errors_and_empty_limits_are_unknown() {
    let home = fixture("personal-errors");
    for (response, expected) in [
        (
            r#"{"id":3,"result":{"rateLimits":{"primary":{"usedPercent":101}}}}"#,
            "failed",
        ),
        (
            r#"{"id":3,"result":{"rateLimits":{"primary":{"usedPercent":-1}}}}"#,
            "failed",
        ),
        (
            r#"{"id":3,"result":{"rateLimits":{"primary":{"usedPercent":2.5}}}}"#,
            "failed",
        ),
        (
            r#"{"id":3,"error":{"message":"private token details"}}"#,
            "failed",
        ),
        (
            r#"{"id":3,"result":{"rateLimits":{"primary":null,"secondary":null}}}"#,
            "missing",
        ),
    ] {
        std::fs::write(home.join("limits.json"), response).unwrap();
        let output = run(
            &home,
            &["usage", "--refresh", "--provider", "codex", "--json"],
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("private token details"));
        let usage = json(output);
        let work = usage["meters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["account"] == "work")
            .unwrap();
        assert_eq!(work["state"], expected);
        assert!(work["used_pct"].is_null());
    }
}

#[test]
fn refresh_finishes_before_the_swift_timeout_and_reaps_its_process() {
    let home = fixture("spare-deadline");
    tool(
        &home,
        "codex",
        r#"echo "$*" >> "$HOME/processes"
echo "$$" > "$HOME/child-pid"
exec /bin/sleep 25"#,
    );
    let start = std::time::Instant::now();
    let usage = json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    assert!(start.elapsed() >= std::time::Duration::from_secs(17));
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
    assert!(
        usage["meters"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["state"] == "failed"
                && row["used_pct"].is_null()
                && row["reason"] == "Codex usage read failed")
    );
    assert_eq!(
        std::fs::read_to_string(home.join("processes"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let pid = std::fs::read_to_string(home.join("child-pid")).unwrap();
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn cache_home_inside_a_provider_home_is_rejected_before_a_write() {
    let home = fixture("work-outside");
    let provider_home = home.join(".codex-work");
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &provider_home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .args(["usage", "--refresh", "--provider", "codex", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside provider homes"));
    assert!(!provider_home.join(".swarm").exists());
}

#[test]
fn account_and_usage_reads_share_validation_and_keep_failed_windows() {
    let home = fixture("work-validation");
    json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let path = home.join(".swarm/codex-usage.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for (field, value) in [
        ("state", serde_json::json!("unknown-state")),
        ("used_pct", serde_json::json!(150)),
        ("window_minutes", serde_json::json!(0)),
        ("reset_time_seconds", serde_json::json!(-1)),
    ] {
        let mut cache = original.clone();
        let work = cache["accounts"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["home"].as_str().unwrap().ends_with("/.codex-work"))
            .unwrap();
        work["meters"][0][field] = value;
        let mut ok = work["meters"][0].clone();
        ok["state"] = "fresh".into();
        ok["used_pct"] = 20.into();
        ok["window_minutes"] = 300.into();
        ok["reset_time_seconds"] = 1791540000i64.into();
        ok["window"] = "secondary".into();
        work["meters"].as_array_mut().unwrap().push(ok);
        std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
        let list = json(run(&home, &["accounts", "--provider", "codex", "--json"]));
        let account = list["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "work")
            .unwrap();
        assert_eq!(account["usage_state"], "failed", "field {field}");
        assert!(account["remaining_pct"].is_null(), "field {field}");
        assert_ne!(list["auto"], "work");
        let usage = json(run(&home, &["usage", "--json"]));
        let windows: Vec<_> = usage["meters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["provider"] == "codex" && row["account"] == "work")
            .collect();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0]["state"], "failed");
        assert_eq!(windows[1]["state"], "fresh");
    }
    std::fs::write(&path, "invalid cache").unwrap();
    let list = json(run(&home, &["accounts", "--provider", "codex", "--json"]));
    assert!(
        list["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "work" && row["usage_state"] == "failed")
    );
}

#[test]
fn claude_failed_window_and_missing_source_match_account_reads() {
    let home = fixture("personal-validation");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for bad in [
        serde_json::json!({"provider":"claude","label":"cl·owner@example.test","window":"7d","state":"failed"}),
        serde_json::json!({"provider":"claude","label":"cl·owner@example.test","window":"7d","state":"unknown","pct":20}),
        serde_json::json!({"provider":"claude","label":"cl·owner@example.test","window":"7d","state":"ok","pct":150}),
        serde_json::json!({"provider":"claude","label":"cl·owner@example.test","window":"7d","state":"ok","pct":2.5}),
    ] {
        let input = serde_json::json!([bad, {"provider":"claude","label":"cl·owner@example.test","window":"5h","pct":20,"state":"ok","asOf":now}]);
        tool(&home, "yelo", &format!("echo '{input}'"));
        let list = json(run(&home, &["accounts", "--provider", "claude", "--json"]));
        assert_eq!(list["accounts"][0]["usage_state"], "failed", "{input}");
        assert!(list["accounts"][0]["remaining_pct"].is_null());
        let usage = json(run(&home, &["usage", "--json"]));
        let windows: Vec<_> = usage["meters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["provider"] == "claude")
            .collect();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0]["state"], "failed");
        assert_eq!(windows[1]["state"], "fresh");
    }
    std::fs::remove_file(home.join("bin/yelo")).unwrap();
    let list = json(run(&home, &["accounts", "--provider", "claude", "--json"]));
    assert_eq!(list["accounts"][0]["usage_state"], "failed");
}

#[test]
fn failed_refresh_retains_good_samples_and_their_age_as_stale() {
    let home = fixture("spare-retained-cache");
    let first = json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let before = first["meters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["account"] == "work")
        .unwrap();
    std::fs::write(
        home.join("limits.json"),
        r#"{"id":3,"error":{"message":"secret error"}}"#,
    )
    .unwrap();
    let failed = json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let stale = failed["meters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["account"] == "work")
        .unwrap();
    assert_eq!(stale["state"], "stale");
    assert_eq!(stale["used_pct"], before["used_pct"]);
    assert_eq!(stale["as_of_seconds"], before["as_of_seconds"]);
    assert!(!stale.to_string().contains("secret error"));
    let cached = json(run(&home, &["usage", "--json"]));
    assert!(
        cached["meters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["account"] == "work"
                && row["state"] == "stale"
                && row["used_pct"] == 30)
    );
}

#[test]
fn refused_usage_cache_keeps_native_accounts_with_failed_usage() {
    let home = fixture("spare-refused-cache");
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("CLAUDE_CONFIG_DIR", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .args(["accounts", "--provider", "codex", "--json"])
        .output()
        .unwrap();
    let list = json(output);
    assert_eq!(list["state"], "ready");
    let work = list["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "work")
        .unwrap();
    assert_eq!(work["auth_state"], "signed_in");
    assert_eq!(work["usage_state"], "failed");
    assert!(work["remaining_pct"].is_null());
    assert!(!home.join(".swarm/codex-usage.json").exists());
}

#[test]
fn failed_refresh_keeps_failed_windows_beside_retained_good_windows() {
    let home = fixture("work-mixed-retention");
    json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let path = home.join(".swarm/codex-usage.json");
    let mut cache: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let work = cache["accounts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["home"].as_str().unwrap().ends_with("/.codex-work"))
        .unwrap();
    let mut failed = work["meters"][0].clone();
    failed["state"] = "failed".into();
    failed["used_pct"] = serde_json::Value::Null;
    failed["window"] = "secondary".into();
    work["meters"].as_array_mut().unwrap().push(failed);
    std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
    std::fs::write(
        home.join("limits.json"),
        r#"{"id":3,"error":{"message":"quota-unavailable"}}"#,
    )
    .unwrap();
    let refreshed = json(run(
        &home,
        &["usage", "--refresh", "--provider", "codex", "--json"],
    ));
    let rows: Vec<_> = refreshed["meters"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["account"] == "work")
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["state"], "stale");
    assert_eq!(rows[1]["state"], "failed");
    let list = json(run(&home, &["accounts", "--provider", "codex", "--json"]));
    let work = list["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "work")
        .unwrap();
    assert_eq!(work["usage_state"], "failed");
    assert!(work["remaining_pct"].is_null());
}
