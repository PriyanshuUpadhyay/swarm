use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TOKEN: &str = "fake-secret-token-keep-private";

fn tool(home: &Path, name: &str, body: &str) {
    let path = home.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture(name: &str) -> PathBuf {
    let home =
        std::env::temp_dir().join(format!("swarm-claude-usage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".swarm")).unwrap();
    std::fs::write(
        home.join(".curlrc"),
        format!(
            "location\ntrace-ascii = \"{}/curl-trace\"\n",
            home.display()
        ),
    )
    .unwrap();
    tool(&home, "claude", "echo '{\"loggedIn\":true}'");
    tool(
        &home,
        "security",
        &format!(
            "printf '%s\\n' \"$*\" >> \"$HOME/security-args\"\necho '{TOKEN}' >&2\necho '{{\"claudeAiOauth\":{{\"accessToken\":\"{TOKEN}\"}}}}'"
        ),
    );
    tool(
        &home,
        "curl",
        "printf '%s\\n' \"$*\" >> \"$HOME/curl-args\"\nexec /usr/bin/curl \"$@\"",
    );
    home
}

fn run(home: &Path, url: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env("USER", "test-owner")
        .env("SWARM_CLAUDE_CMD", home.join("claude"))
        .env("SWARM_SECURITY_CMD", home.join("security"))
        .env("SWARM_CURL_CMD", home.join("curl"))
        .env("SWARM_CLAUDE_USAGE_URL", url)
        .env("PATH", "/usr/bin:/bin")
        .args(args)
        .output()
        .unwrap()
}

fn server(status: &str, headers: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/usage", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (url, handle)
}

fn usage(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(TOKEN));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(TOKEN));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn refresh_uses_stdin_headers_and_nonsecret_cache() {
    let home = fixture("fresh");
    let (url, request) = server(
        "200 OK",
        "",
        r#"{"five_hour":{"utilization":12.6,"resets_at":"2026-10-12T01:02:03.000+00:00"},"seven_day":{"used_percentage":25,"resets_at":1791766923},"limits":[{"kind":"weekly_scoped","scope":{"model":{"display_name":"Fable"}},"utilization":90}]}"#,
    );
    let output = run(
        &home,
        &url,
        &["usage", "--refresh", "--provider", "claude", "--json"],
    );
    let value = usage(&output);
    assert_eq!(value["schema"], "swarm.usage.v1");
    assert_eq!(value["meters"][0]["state"], "fresh");
    assert_eq!(value["meters"][0]["window"], "5h");
    assert_eq!(value["meters"][0]["used_pct"], 13);
    assert_eq!(value["meters"][0]["reset_time_seconds"], 1791766923);
    assert_eq!(value["meters"][1]["window"], "7d");
    assert_eq!(value["meters"][2]["window"], "fb");
    assert_eq!(value["meters"][2]["window_minutes"], 10080);
    let request = request.join().unwrap();
    assert!(request.contains(&format!("Authorization: Bearer {TOKEN}")));
    assert!(request.contains("anthropic-beta: oauth-2025-04-20"));
    assert!(!home.join("curl-trace").exists());
    for file in ["curl-args", "security-args", ".swarm/claude-usage.json"] {
        assert!(
            !std::fs::read_to_string(home.join(file))
                .unwrap()
                .contains(TOKEN)
        );
    }
    assert!(
        std::fs::read_to_string(home.join("security-args"))
            .unwrap()
            .contains("-s Claude Code-credentials -a test-owner -w")
    );
    let cached = usage(&run(&home, &url, &["usage", "--json"]));
    assert_eq!(cached["schema"], "swarm.usage.v1");
    assert!(
        cached["meters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["provider"] == "claude"
                && row["state"] == "fresh"
                && row["source"] == "claude_api")
    );
    let accounts = usage(&run(
        &home,
        &url,
        &["accounts", "--provider", "claude", "--json"],
    ));
    assert_eq!(accounts["accounts"][0]["usage_source"], "claude_api");
    assert_eq!(
        // The --write-out argument holds a newline, so count calls by their one --url flag.
        std::fs::read_to_string(home.join("curl-args"))
            .unwrap()
            .matches("--url")
            .count(),
        1
    );
}

#[test]
fn auth_failure_clears_old_usage_and_keeps_sign_in_reason() {
    for code in ["401 Unauthorized", "403 Forbidden"] {
        let home = fixture(&code[..3]);
        let (url, request) = server("200 OK", "", r#"{"five_hour":{"utilization":20}}"#);
        usage(&run(
            &home,
            &url,
            &["usage", "--refresh", "--provider", "claude", "--json"],
        ));
        request.join().unwrap();
        let (url, request) = server(code, "", TOKEN);
        let value = usage(&run(
            &home,
            &url,
            &["usage", "--refresh", "--provider", "claude", "--json"],
        ));
        request.join().unwrap();
        assert_eq!(value["meters"][0]["state"], "failed");
        assert_eq!(value["meters"][0]["reason"], "Sign in again");
        assert_eq!(value["meters"][0]["reason_code"], "relogin_required");
        assert!(value["meters"][0]["used_pct"].is_null());
        let cached = usage(&run(&home, &url, &["usage", "--json"]));
        assert!(
            cached["meters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["provider"] == "claude"
                    && row["reason"] == "Sign in again"
                    && row["reason_code"] == "relogin_required")
        );
        assert!(
            !std::fs::read_to_string(home.join(".swarm/claude-usage.json"))
                .unwrap()
                .contains(TOKEN)
        );
    }
}

#[test]
fn a_signed_out_account_keeps_its_reason_in_a_cached_read() {
    let home = fixture("signed-out");
    tool(&home, "claude", "echo '{\"loggedIn\":false}'");
    let refreshed = usage(&run(
        &home,
        "http://127.0.0.1:9/unused",
        &["usage", "--refresh", "--provider", "claude", "--json"],
    ));
    assert_eq!(refreshed["meters"][0]["reason"], "Account is signed out");
    let cached = usage(&run(
        &home,
        "http://127.0.0.1:9/unused",
        &["usage", "--json"],
    ));
    let row = cached["meters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["provider"] == "claude")
        .unwrap();
    assert_eq!(row["state"], "missing");
    assert_eq!(row["reason"], "Account is signed out");
    assert_eq!(row["reason_code"], "no_credentials");
}

#[test]
fn redirects_are_not_followed() {
    let home = fixture("redirect");
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    destination.set_nonblocking(true).unwrap();
    let (url, request) = server(
        "302 Found",
        &format!(
            "Location: http://{}/secret\r\n",
            destination.local_addr().unwrap()
        ),
        TOKEN,
    );
    let value = usage(&run(
        &home,
        &url,
        &["usage", "--refresh", "--provider", "claude", "--json"],
    ));
    request.join().unwrap();
    assert_eq!(value["meters"][0]["state"], "failed");
    assert_eq!(
        destination.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn curl_timeout_uses_the_remaining_deadline() {
    let home = fixture("deadline");
    tool(
        &home,
        "security",
        &format!(
            "/bin/sleep 0.2\necho '{TOKEN}' >&2\necho '{{\"claudeAiOauth\":{{\"accessToken\":\"{TOKEN}\"}}}}'"
        ),
    );
    tool(
        &home,
        "curl",
        "printf '%s\\n' \"$*\" > \"$HOME/curl-args\"\necho \"$$\" > \"$HOME/curl-pid\"\nexec /bin/sleep 25",
    );
    let start = std::time::Instant::now();
    let value = usage(&run(
        &home,
        "http://127.0.0.1:1/usage",
        &["usage", "--refresh", "--provider", "claude", "--json"],
    ));
    assert!(start.elapsed() >= std::time::Duration::from_secs(17));
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
    let args = std::fs::read_to_string(home.join("curl-args")).unwrap();
    let words: Vec<_> = args.split_whitespace().collect();
    let index = words.iter().position(|word| *word == "--max-time").unwrap();
    let seconds: f64 = words[index + 1].parse().unwrap();
    assert!(seconds > 0.0 && seconds < 17.9, "{seconds}");
    assert_eq!(value["meters"][0]["state"], "failed");
    let pid = std::fs::read_to_string(home.join("curl-pid")).unwrap();
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
fn current_config_home_uses_its_own_keychain_service() {
    let home = fixture("current-keychain");
    // swarm writes the auth header to curl's stdin, so a fake that exits unread breaks the pipe.
    tool(
        &home,
        "curl",
        "cat >/dev/null; echo '{\"five_hour\":{\"utilization\":10}}'; printf '\\n200'",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("USER", "test-owner")
        .env("CLAUDE_CONFIG_DIR", "/Users/example/.claude-work")
        .env("SWARM_CLAUDE_CMD", home.join("claude"))
        .env("SWARM_SECURITY_CMD", home.join("security"))
        .env("SWARM_CURL_CMD", home.join("curl"))
        .env("SWARM_CLAUDE_USAGE_URL", "http://unused.invalid")
        .env("PATH", "/usr/bin:/bin")
        .args(["usage", "--refresh", "--provider", "claude", "--json"])
        .output()
        .unwrap();
    let value = usage(&output);
    assert!(
        value["meters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|meter| meter["account"] == "current" && meter["state"] == "fresh")
    );
    let args = std::fs::read_to_string(home.join("security-args")).unwrap();
    assert!(
        args.contains("-s Claude Code-credentials-dd1118a7 -a test-owner -w"),
        "{args}"
    );
}

fn security_with_expiry(home: &Path, expires_at_millis: i64) {
    tool(
        home,
        "security",
        &format!(
            "echo '{{\"claudeAiOauth\":{{\"accessToken\":\"{TOKEN}\",\"expiresAt\":{expires_at_millis}}}}}'"
        ),
    );
}

#[test]
fn an_expired_token_makes_no_request_and_keeps_its_reason_in_a_cached_read() {
    let home = fixture("expired");
    let now_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    security_with_expiry(&home, now_millis - 60_000);
    let value = usage(&run(
        &home,
        "http://127.0.0.1:9/unused",
        &["usage", "--refresh", "--provider", "claude", "--json"],
    ));
    assert_eq!(value["meters"].as_array().unwrap().len(), 1);
    assert_eq!(value["meters"][0]["state"], "failed");
    assert_eq!(value["meters"][0]["reason"], "Token expired");
    assert_eq!(value["meters"][0]["reason_code"], "token_expired");
    assert!(!home.join("curl-args").exists());
    let cached = usage(&run(
        &home,
        "http://127.0.0.1:9/unused",
        &["usage", "--json"],
    ));
    assert!(
        cached["meters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["provider"] == "claude"
                && row["reason"] == "Token expired"
                && row["reason_code"] == "token_expired")
    );
}

#[test]
fn a_rejected_live_token_still_asks_to_sign_in_again() {
    let home = fixture("live-401");
    let now_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    security_with_expiry(&home, now_millis + 3_600_000);
    let (url, request) = server("401 Unauthorized", "", "");
    let value = usage(&run(
        &home,
        &url,
        &["usage", "--refresh", "--provider", "claude", "--json"],
    ));
    request.join().unwrap();
    assert_eq!(value["meters"][0]["reason"], "Sign in again");
}

#[test]
fn each_reason_on_a_claude_path_carries_its_code_and_a_meter_with_no_reason_has_a_null_code() {
    let home = fixture("reason-codes");
    let claude = |value: &serde_json::Value| {
        value["meters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["provider"] == "claude")
            .unwrap()
            .clone()
    };
    let cached = usage(&run(
        &home,
        "http://127.0.0.1:9/unused",
        &["usage", "--json"],
    ));
    let row = claude(&cached);
    assert_eq!(
        (&row["state"], &row["reason"], &row["reason_code"]),
        (
            &"missing".into(),
            &"No cached usage".into(),
            &"no_cached_usage".into()
        )
    );
    let refresh = |status: &str, body: &str| {
        let (url, request) = server(status, "", body);
        let value = usage(&run(
            &home,
            &url,
            &["usage", "--refresh", "--provider", "claude", "--json"],
        ));
        request.join().unwrap();
        claude(&value)
    };
    let row = refresh("500 Internal Server Error", "");
    assert_eq!(
        (&row["state"], &row["reason"], &row["reason_code"]),
        (
            &"failed".into(),
            &"Claude usage read failed".into(),
            &"read_failed".into()
        )
    );
    let row = refresh("200 OK", r#"{"five_hour":{"utilization":20}}"#);
    assert_eq!(row["state"], "fresh");
    assert_eq!(row.get("reason"), Some(&serde_json::Value::Null));
    assert_eq!(row.get("reason_code"), Some(&serde_json::Value::Null));
    let row = refresh("500 Internal Server Error", "");
    assert_eq!(
        (&row["state"], &row["reason"], &row["reason_code"]),
        (
            &"stale".into(),
            &"Usage refresh failed; old reading".into(),
            &"refresh_failed".into()
        )
    );
}

#[test]
fn the_openusage_export_reads_the_cache_only_and_an_unknown_format_fails() {
    let home = fixture("openusage");
    let (url, request) = server("200 OK", "", r#"{"five_hour":{"utilization":20}}"#);
    usage(&run(
        &home,
        &url,
        &["usage", "--refresh", "--provider", "claude", "--json"],
    ));
    request.join().unwrap();
    std::fs::remove_file(home.join("curl-args")).unwrap();
    let output = run(
        &home,
        "http://127.0.0.1:9/unused",
        &["usage", "--format", "openusage"],
    );
    let value = usage(&output);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains(&*home.to_string_lossy()));
    assert!(!home.join("curl-args").exists());
    assert_eq!(value["schema"], "openusage.limits.v1");
    let generated = value["generatedAt"].as_str().unwrap();
    assert!(
        generated.ends_with(".000Z") && generated.len() == 24,
        "{generated}"
    );
    let claude = &value["providers"]["claude"];
    assert_eq!(claude["displayName"], "Claude");
    assert_eq!(claude["stale"], false);
    assert_eq!(claude["resources"]["session"]["used"], 20);
    assert_eq!(claude["resources"]["session"]["windowSeconds"], 18000);
    assert_eq!(value["errors"], serde_json::json!([]));
    let other = run(
        &home,
        "http://127.0.0.1:9/unused",
        &["usage", "--format", "csv"],
    );
    assert!(!other.status.success());
    assert_eq!(
        String::from_utf8_lossy(&other.stderr).trim(),
        "swarm: unknown usage format csv; the one format is openusage"
    );
}

#[test]
fn a_usage_read_does_not_read_the_roles_config() {
    let home = fixture("no-roles-read");
    let roles = home.join(".config/agent-routing/roles.json");
    std::fs::create_dir_all(roles.parent().unwrap()).unwrap();
    std::fs::write(
        &roles,
        r#"{"routes":{"chat":["claude-opus"]},"runners":{"claude-opus":{"provider":"claude","model":"opus","effort":"high"}}}"#,
    )
    .unwrap();
    let profiles = home.join(".swarm/profiles.json");
    let output = run(&home, "http://127.0.0.1:9/unused", &["usage", "--json"]);
    usage(&output);
    assert!(!profiles.exists());
    // The listing does read it, so the fixture is a real import.
    let listing = run(
        &home,
        "http://127.0.0.1:9/unused",
        &["accounts", "--provider", "claude", "--json"],
    );
    assert!(listing.status.success());
    assert!(profiles.exists());
    // With the old file newer than the import, only a reader of the config says so.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&roles, std::fs::read(&roles).unwrap()).unwrap();
    for args in [
        &["usage", "--json"][..],
        &["usage", "--refresh", "--provider", "claude", "--json"][..],
    ] {
        let output = run(&home, "http://127.0.0.1:9/unused", args);
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("changed after import"),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let listing = run(
        &home,
        "http://127.0.0.1:9/unused",
        &["accounts", "--provider", "claude", "--json"],
    );
    assert!(String::from_utf8_lossy(&listing.stderr).contains("changed after import"));
}
