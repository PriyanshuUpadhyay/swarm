use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        .args(args);
    command
}

fn env(home: &Path) -> String {
    std::fs::read_to_string(home.join("child-env")).unwrap()
}

#[test]
fn a_worker_run_execs_claude_with_exact_args_input_output_and_status() {
    let home = fixture("worker-exec");
    tool(
        &home,
        "claude",
        r#"if [ "$*" = 'auth status' ]; then echo '{"loggedIn":true}'; else
/usr/bin/env > "$HOME/child-env"
printf '%s\0' "$@" > "$HOME/child-args"
printf '%s' "$$" > "$HOME/child-pid"
IFS= read -r input
printf 'child stdout %s\n' "$input"
printf 'child stderr\n' >&2
exit 23
fi"#,
    );
    let args = [
        "--model",
        "opus",
        "two words",
        "",
        "line one\nline two",
        "$(touch injected)",
        "--account",
        "provider-arg",
    ];
    let mut cli_args = vec!["run", "claude", "--account", "work", "--"];
    cli_args.extend(args);
    let mut child = command(&home, &cli_args)
        .env("SWARM_AGENT_ID", "worker")
        .env("HERDR_AGENT_PANE", "1")
        .env("CLAUDE_CONFIG_DIR", "/wrong")
        .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/wrong-store")
        .env("AGENT_PROFILE_LABEL", "wrong")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"hello terminal\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(23),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"child stdout hello terminal\n");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "account work\nchild stderr\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("child-pid")).unwrap(),
        pid.to_string()
    );
    let bytes = std::fs::read(home.join("child-args")).unwrap();
    let actual: Vec<_> = bytes
        .strip_suffix(&[0])
        .unwrap()
        .split(|byte| *byte == 0)
        .map(|arg| std::str::from_utf8(arg).unwrap())
        .collect();
    assert_eq!(actual, args);
    assert!(!home.join("injected").exists());
    let env = env(&home);
    assert!(
        !env.lines()
            .any(|line| line.starts_with("CLAUDE_CONFIG_DIR=")),
        "{env}"
    );
    assert!(env.lines().any(|line| line
        == format!(
            "CLAUDE_SECURESTORAGE_CONFIG_DIR={}/.claude-work",
            home.display()
        )));
    assert!(env.lines().any(|line| line == "AGENT_PROFILE_LABEL=work"));
    assert!(
        env.lines()
            .any(|line| line == format!("PWD={}", home.display()))
    );
    assert!(!home.join(".swarm/swarm.db").exists());
    assert!(!home.join(".swarm/runs").exists());
}

#[test]
fn auto_run_and_named_run_apply_each_providers_launch_env() {
    let home = fixture("auto");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for provider in ["claude", "codex"] {
        let path = home.join(if provider == "claude" {
            ".claude/.profiles/work"
        } else {
            ".codex-work"
        });
        let cache = serde_json::json!({"accounts":[{"home":path,"meters":[{"provider":provider,"account":"work","label":"work","used_pct":10,"state":"fresh","as_of_seconds":now}]}]});
        std::fs::write(
            home.join(format!(".swarm/{provider}-usage.json")),
            cache.to_string(),
        )
        .unwrap();
        for selection in [None, Some("auto"), Some("work")] {
            let mut args = vec!["run", provider];
            if let Some(selection) = selection {
                args.extend(["--account", selection]);
            }
            let output = command(&home, &args)
                .env("CODEX_HOME", "/wrong")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stderr).starts_with("account work\n"));
            let env = env(&home);
            let expected = if provider == "codex" {
                format!("CODEX_HOME={}", path.display())
            } else {
                format!(
                    "CLAUDE_SECURESTORAGE_CONFIG_DIR={}/.claude-work",
                    home.display()
                )
            };
            assert!(env.lines().any(|line| line == expected), "{env}");
        }
    }
}

#[test]
fn unknown_and_signed_out_accounts_never_execute_the_cli() {
    let home = fixture("refuse");
    for provider in ["claude", "codex"] {
        for (account, message) in [("absent", "unknown"), ("personal", "signed out")] {
            let output = command(&home, &["run", provider, "--account", account])
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(message),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!home.join("child-env").exists());
        }
    }
}

#[test]
fn auto_without_a_usable_account_clears_overrides_and_keeps_the_cli_login() {
    let home = fixture("fallback");
    tool(
        &home,
        "claude",
        "if [ \"$*\" = 'auth status' ]; then exit 1; else /usr/bin/env > \"$HOME/child-env\"; fi",
    );
    let output = command(&home, &["run", "claude"])
        .env("CLAUDE_CONFIG_DIR", "/wrong")
        .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/wrong-store")
        .env("AGENT_PROFILE_LABEL", "wrong")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "swarm: no automatic account for claude; claude uses its own login\n"
    );
    let env = env(&home);
    for key in [
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_SECURESTORAGE_CONFIG_DIR",
        "AGENT_PROFILE_LABEL",
    ] {
        assert!(
            !env.lines().any(|line| line.starts_with(&format!("{key}="))),
            "{env}"
        );
    }
}

#[test]
fn invalid_run_syntax_stops_before_exec_and_a_missing_cli_has_an_error() {
    let home = fixture("syntax");
    for args in [
        &["run"][..],
        &["run", "agy"],
        &["run", "unknown"],
        &["run", "claude", "--account"],
        &["run", "claude", "--account", "auto", "--account", "work"],
        &["run", "claude", "--model", "opus"],
    ] {
        let output = command(&home, args).output().unwrap();
        assert!(!output.status.success());
        if args == ["run"] {
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("run <claude|codex> [--account <auto|NAME>] [-- <args>...]")
            );
        }

        assert!(!home.join("child-env").exists());
    }
    let output = command(&home, &["run", "claude"])
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot run claude"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
