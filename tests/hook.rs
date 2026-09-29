use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-hook-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".swarm")).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// Runs the built binary with only HOME, PATH, and the given variables set.
fn swarm(home: &Path, env: &[(&str, &str)], args: &[&str], stdin: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("SWARM_ADAPTER", "tmux")
        .current_dir(home)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn coder_state(home: &Path, session: &str) -> serde_json::Value {
    let listing = swarm(
        home,
        &[("SWARM_SESSION_ID", session)],
        &["agents", "--json"],
        "",
    );
    let listing: serde_json::Value = serde_json::from_str(&stdout(&listing)).unwrap();
    let coder = &listing["agents"][0];
    assert_eq!(coder["id"], "coder");
    coder.clone()
}

#[test]
fn a_claude_permission_request_shows_the_coder_waiting() {
    let home = scratch("waiting");
    let session = stdout(&swarm(&home, &[], &["session", "new", "lane"], ""))
        .trim()
        .to_string();
    let in_session = [("SWARM_SESSION_ID", session.as_str())];
    let added = swarm(&home, &in_session, &["agent", "add", "coder", "coder"], "");
    assert!(added.status.success());

    let coder = coder_state(&home, &session);
    for field in ["state", "state_at_s", "state_source", "state_detail"] {
        assert!(coder[field].is_null(), "{field}: {coder}");
    }

    let permission = r#"{"hook_event_name":"PermissionRequest","session_id":"abc"}"#;
    let outside = swarm(&home, &[], &["hook", "claude"], permission);
    assert!(outside.status.success());
    assert_eq!(stdout(&outside), "{}\n");
    assert!(coder_state(&home, &session)["state"].is_null());

    let as_coder = [
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "coder"),
    ];
    let reported = swarm(&home, &as_coder, &["hook", "claude"], permission);
    assert!(reported.status.success());
    assert_eq!(stdout(&reported), "{}\n");
    let coder = coder_state(&home, &session);
    assert_eq!(coder["state"], "waiting");
    assert_eq!(coder["state_source"], "hook");
    assert!(coder["state_at_s"].as_i64().unwrap() > 0);

    let broken = swarm(&home, &as_coder, &["hook", "claude"], "not json");
    assert!(broken.status.success());
    assert_eq!(stdout(&broken), "{}\n");
    assert!(!broken.stderr.is_empty());
}
