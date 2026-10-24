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
    std::fs::write(home.join("limits.json"), r#"{"id":3,"result":{"rateLimits":{"primary":{"usedPercent":30,"windowDurationMins":300,"resetsAt":1791540000},"secondary":null},"rateLimitResetCredits":{"availableCount":2,"credits":[{"id":"c1","resetType":"codexRateLimits","status":"available","grantedAt":1791000000,"expiresAt":null},{"id":"c2","resetType":"codexRateLimits","status":"available","grantedAt":1791000000,"expiresAt":4102444800}]}}}"#).unwrap();
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
        // The account pick budget a debug build takes, long enough for fake CLIs on a loaded Mac.
        .env("SWARM_TEST_ACCOUNT_PICK_MS", "30000")
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
    assert_eq!(personal["reason_code"], "no_source");
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
fn invalid_claude_home_has_no_usage_with_matching_missing_or_failed_cache() {
    let home = fixture("personal-invalid-usage");
    std::fs::create_dir_all(home.join(".claude/.profiles/Personal")).unwrap();
    json(run(&home, &["accounts", "--provider", "claude", "--json"]));
    for cache in [r#"{"accounts":[]}"#, "invalid cache"] {
        std::fs::create_dir_all(home.join(".swarm")).unwrap();
        std::fs::write(home.join(".swarm/claude-usage.json"), cache).unwrap();
        let list = json(run(&home, &["accounts", "--provider", "claude", "--json"]));
        let account = list["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "Personal")
            .unwrap();
        let usage = json(run(&home, &["usage", "--json"]));
        let invalid: Vec<_> = usage["meters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["provider"] == "claude" && row["account"] == "Personal")
            .collect();
        assert_eq!(invalid.len(), 1);
        assert_eq!(invalid[0]["state"], "no_source");
        assert_eq!(invalid[0]["reason"], account["summary"]);
        assert_eq!(invalid[0]["reason_code"], "no_source");
        assert!(invalid[0]["source"].is_null());
        assert!(invalid[0]["used_pct"].is_null());
    }
}

fn claude_cache(home: &Path, meters: serde_json::Value) {
    json(run(home, &["accounts", "--provider", "claude", "--json"]));
    std::fs::create_dir_all(home.join(".swarm")).unwrap();
    let cache = serde_json::json!({"accounts":[{"home":home.join(".claude"),"meters":meters}]});
    std::fs::write(home.join(".swarm/claude-usage.json"), cache.to_string()).unwrap();
}

fn claude_meter(
    window: &str,
    percent: serde_json::Value,
    state: &str,
    now: u64,
) -> serde_json::Value {
    serde_json::json!({"provider":"claude","account":"default","label":"default","window":window,"window_minutes":10080,"used_pct":percent,"reset_time_seconds":null,"state":state,"source":"claude_api","reason":null,"as_of_seconds":now})
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

#[path = "native_usage/cache_behavior.rs"]
mod cache_behavior;
