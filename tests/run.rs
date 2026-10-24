use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "run/execution.rs"]
mod execution;
#[path = "run/model_usage.rs"]
mod model_usage;
#[path = "run/selection.rs"]
mod selection;

fn fixture(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("swarm-run-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    for directory in [
        ".claude/.profiles/work",
        ".claude/.profiles/personal",
        ".codex-work",
        ".codex-personal",
        ".swarm",
    ] {
        std::fs::create_dir_all(home.join(directory)).unwrap();
    }
    std::fs::write(home.join(".swarm/swarm-home"), "swarm\n").unwrap();
    tool(
        &home,
        "claude",
        r#"if [ "$*" = 'auth status' ]; then
case "$CLAUDE_CONFIG_DIR" in */personal) echo '{"loggedIn":false}';; *) echo '{"loggedIn":true}';; esac
else
/usr/bin/env > "$HOME/child-env"
printf '%s\0' "$@" > "$HOME/child-args"
printf '%s' "$$" > "$HOME/child-pid"
printf 'child stdout\n'
printf 'child stderr\n' >&2
fi"#,
    );
    tool(
        &home,
        "codex",
        &format!(
            r#"if [ "$*" = 'app-server --listen stdio://' ]; then
{}
else
/usr/bin/env > "$HOME/child-env"
printf '%s\0' "$@" > "$HOME/child-args"
fi"#,
            include_str!("fixtures/accounts/work-app-server.sh")
        ),
    );
    std::fs::canonicalize(home).unwrap()
}

fn tool(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn command(home: &Path, args: &[&str]) -> Command {
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
    command
}

fn env(home: &Path) -> String {
    std::fs::read_to_string(home.join("child-env")).unwrap()
}

#[test]
fn a_named_pick_reads_every_account_within_the_pick_deadline() {
    let home = fixture("slow-logins");
    for name in ["a", "b", "c"] {
        std::fs::create_dir_all(home.join(format!(".codex-{name}"))).unwrap();
    }
    // Each login read takes 1.5 s, so five reads in a row pass the 6 s pick deadline.
    tool(
        &home,
        "codex",
        &format!(
            "sleep 1.5\n{}",
            include_str!("fixtures/accounts/work-app-server.sh")
        ),
    );
    for name in ["work", "a", "b", "c"] {
        let output = command(
            &home,
            &[
                "accounts",
                "pick",
                "--provider",
                "codex",
                "--account",
                name,
                "--json",
            ],
        )
        .env("SWARM_TEST_ACCOUNT_PICK_MS", "6000")
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
