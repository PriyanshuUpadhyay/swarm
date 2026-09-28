use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DENIED: &str =
    r#"Error: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-launch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::create_dir_all(dir.join(".swarm")).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// A stand-in for a CLI that swarm calls, placed first on the test's PATH.
fn tool(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Runs the built binary with only HOME, PATH, and the given variables set.
fn swarm(home: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .env_clear()
        .env("HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .current_dir(home)
        .args(args);
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_denied_herdr_socket_refuses_every_call_that_starts_a_run() {
    let home = scratch("socket");
    let herdr = [("SWARM_ADAPTER", "herdr")];
    tool(&home, "herdr", &format!("echo '{DENIED}' >&2; exit 1"));
    for args in [
        &["session", "new", "lane"][..],
        &["agent", "add", "orchestrator", "orchestrator"],
    ] {
        let output = swarm(&home, &herdr, args);
        assert!(!output.status.success(), "{args:?} passed");
        assert!(
            stderr(&output).contains("denies the Herdr socket"),
            "{args:?}: {}",
            stderr(&output)
        );
    }

    tool(&home, "herdr", "exit 0");
    let output = swarm(&home, &herdr, &["session", "new", "lane"]);
    assert!(output.status.success(), "{}", stderr(&output));
}
