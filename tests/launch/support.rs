// Shared launch fixtures and command helpers.

use super::*;

pub(super) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-launch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::create_dir_all(dir.join(".swarm")).unwrap();
    // The marker says this home is swarm's, so a test can add adapter files before any command
    // (ADR 0036).
    std::fs::write(dir.join(".swarm/swarm-home"), "swarm\n").unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// A stand-in for a CLI that swarm calls, placed first on the test's PATH.
pub(super) fn tool(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Runs the built binary with only HOME, PATH, and the given variables set. SWARM_HOME pins the
/// data to HOME, so a branch build (ADR 0027) reads the same place as a `main` one.
pub(super) fn swarm(home: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .current_dir(home)
        // The account pick budget a debug build takes, long enough for fake CLIs on a loaded Mac.
        .env("SWARM_TEST_ACCOUNT_PICK_MS", "30000")
        .args(args);
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().unwrap()
}

pub(super) fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub(super) fn trusted(config: &Path, dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(config) else {
        return false;
    };
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["projects"][dir.to_string_lossy().as_ref()]["hasTrustDialogAccepted"] == true
}

pub(super) fn chair(session: &str) -> [(&'static str, &str); 3] {
    [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session),
        ("SWARM_AGENT_ID", "orchestrator"),
    ]
}

/// A home with a fake adapter, a `codex` and a `claude` on PATH, and routes `review.deep` to Codex
/// and `chair` to Claude, with one chair session. Returns the chair's env.
pub(super) fn trust_session(home: &Path) -> [(&'static str, String); 3] {
    tool(home, "codex", "true");
    tool(home, "claude", "true");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"review.deep": ["codex-x"], "chair": ["claude-opus"]},
            "runners": {"codex-x": {"provider": "codex", "model": "gpt-6-luna", "effort": "high"},
                        "claude-opus": {"provider": "claude", "model": "opus", "effort": "high"}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let session = swarm(
        home,
        &[("SWARM_ADAPTER", "fake")],
        &["session", "new", "lane"],
    );
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    [
        ("SWARM_ADAPTER", "fake".to_string()),
        ("SWARM_SESSION_ID", session),
        ("SWARM_AGENT_ID", "orchestrator".to_string()),
    ]
}

pub(super) fn git_repo(home: &Path, name: &str) -> PathBuf {
    let dir = home.join(name);
    assert!(
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    dir
}

pub(super) fn recorded_trust(home: &Path) -> Vec<serde_json::Value> {
    let output = swarm(home, &[], &["managed", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["writer"] == "launch.trust" && entry["recorded"] == true)
        .cloned()
        .collect()
}

pub(super) fn account_launch_fixture(role: &str) -> (PathBuf, String, String) {
    let home = scratch(role);
    std::fs::create_dir_all(home.join(".claude/.profiles/work")).unwrap();
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust":"standing"}"#).unwrap();
    tool(
        &home,
        "claude",
        r#"echo "$CLAUDE_CONFIG_DIR" >> "$HOME/native-reads"; /bin/sleep 0.05; echo '{"loggedIn":true}'"#,
    );
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(home.join(".swarm/adapters/fake.conf"), "self = printf chair\nspawn = printf pane-work\nring = true\nlist = true\nclose = true\ncapture = true\n").unwrap();
    let session = swarm(
        &home,
        &[("SWARM_ADAPTER", "fake")],
        &["session", "new", "lane"],
    );
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let cwd = git_repo(&home, "work-project")
        .to_string_lossy()
        .into_owned();
    (home, session, cwd)
}
