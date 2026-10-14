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
fn accounts_pick_prints_the_account_that_run_would_use() {
    let home = fixture("pick");
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
        let (key, value) = if provider == "codex" {
            ("CODEX_HOME", path.display().to_string())
        } else {
            (
                "CLAUDE_SECURESTORAGE_CONFIG_DIR",
                format!("{}/.claude-work", home.display()),
            )
        };
        for selection in [None, Some("auto"), Some("work")] {
            let mut args = vec!["accounts", "pick", "--provider", provider];
            if let Some(selection) = selection {
                args.extend(["--account", selection]);
            }
            args.push("--json");
            let output = command(&home, &args).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let picked: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(picked["schema"], "swarm.account-pick.v1");
            assert_eq!(picked["name"], "work", "{picked}");
            assert_eq!(picked["home"], path.display().to_string());
            assert_eq!(picked["env"][key], value, "{picked}");
            assert!(picked["clear"].as_array().unwrap().iter().any(|k| k == key));
            assert!(!home.join("child-env").exists(), "pick runs no CLI session");
        }
        let output = command(
            &home,
            &[
                "accounts",
                "pick",
                "--provider",
                provider,
                "--account",
                "absent",
                "--json",
            ],
        )
        .output()
        .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unknown"));
    }
    tool(
        &home,
        "claude",
        "if [ \"$*\" = 'auth status' ]; then exit 1; fi",
    );
    std::fs::remove_file(home.join(".swarm/claude-usage.json")).unwrap();
    let output = command(
        &home,
        &["accounts", "pick", "--provider", "claude", "--json"],
    )
    .output()
    .unwrap();
    assert!(output.status.success());
    let picked: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(picked["name"].is_null(), "{picked}");
    assert_eq!(picked["env"], serde_json::json!({}));
    assert!(
        picked["clear"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k == "AGENT_PROFILE_LABEL")
    );
}

#[test]
fn a_named_pick_reads_every_account_within_the_pick_deadline() {
    let home = fixture("slow-logins");
    for name in ["a", "b", "c"] {
        std::fs::create_dir_all(home.join(format!(".codex-{name}"))).unwrap();
    }
    // Each login read takes 0.7 s, so five reads in a row pass the 2 s pick deadline.
    tool(
        &home,
        "codex",
        &format!(
            "sleep 0.7\n{}",
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
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn accounts_pick_finds_the_codex_home_that_holds_a_session() {
    let home = fixture("session-owner");
    let rollout = |codex: &str, folder: &str, started: &str, id: &str, cwd: &Path| {
        let dir = home.join(codex).join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let meta = serde_json::json!({"type":"session_meta","payload":{"id":id,"cwd":cwd}});
        std::fs::write(
            dir.join(format!("rollout-{started}-{id}.jsonl")),
            format!("{meta}\n"),
        )
        .unwrap();
    };
    let (project, other, empty) = (home.join("project"), home.join("other"), home.join("empty"));
    for dir in [&project, &other, &empty] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let first = "0199a000-0000-7000-8000-000000000001";
    let second = "0199a000-0000-7000-8000-000000000002";
    let archived = "0199a000-0000-7000-8000-000000000003";
    rollout(
        ".codex-work",
        "sessions/2026/10/01",
        "2026-10-01T10-00-00",
        first,
        &project,
    );
    rollout(
        ".codex-b",
        "sessions/2026/10/02",
        "2026-10-02T10-00-00",
        second,
        &other,
    );
    rollout(
        ".codex-b",
        "archived_sessions",
        "2026-09-01T10-00-00",
        archived,
        &project,
    );
    let pick = |cwd: &Path, args: &[&str]| {
        let mut all = vec!["accounts", "pick", "--provider", "codex"];
        all.extend(args);
        all.push("--json");
        command(&home, &all).current_dir(cwd).output().unwrap()
    };
    let name = |output: std::process::Output| {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["name"].clone()
    };
    assert_eq!(name(pick(&home, &["--session", first])), "work");
    assert_eq!(
        name(pick(&home, &["--session", &first.to_uppercase()])),
        "work"
    );
    assert_eq!(name(pick(&home, &["--session", archived])), "b");
    assert_eq!(name(pick(&project, &["--last"])), "work");
    assert_eq!(name(pick(&project, &["--last", "--all"])), "b");
    for (cwd, args, message) in [
        (
            &home,
            vec!["--session", "0199a000-0000-7000-8000-00000000000f"],
            "not in any codex account",
        ),
        (&empty, vec!["--last"], "no codex session"),
        (
            &home,
            vec!["--session", first, "--account", "work"],
            "usage",
        ),
    ] {
        let output = pick(cwd, &args);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = command(
        &home,
        &[
            "accounts",
            "pick",
            "--provider",
            "claude",
            "--last",
            "--json",
        ],
    )
    .output()
    .unwrap();
    assert!(!output.status.success());
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

fn model_cache(
    home: &Path,
    provider: &str,
    work_fb: serde_json::Value,
    default_fb: serde_json::Value,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let accounts: Vec<_> = [("work", 10, work_fb), ("default", 50, default_fb)].into_iter().map(|(name, used, fb)| {
        let dir = match (provider, name) { ("claude", "work") => ".claude/.profiles/work", ("claude", _) => ".claude", ("codex", "work") => ".codex-work", _ => ".codex" };
        let mut meters = vec![serde_json::json!({"provider":provider,"account":name,"label":name,"window":"5h","used_pct":used,"state":"fresh","as_of_seconds":now})];
        if !fb.is_null() {
            meters.push(serde_json::json!({"provider":provider,"account":name,"label":name,"window":"fb","used_pct":fb,"state":"fresh","reset_time_seconds":now+3600,"as_of_seconds":now}));
        }
        serde_json::json!({"home":home.join(dir),"meters":meters})
    }).collect();
    std::fs::write(
        home.join(format!(".swarm/{provider}-usage.json")),
        serde_json::json!({"accounts":accounts}).to_string(),
    )
    .unwrap();
}

#[test]
fn run_and_account_json_scope_fable_usage_to_the_model() {
    let home = fixture("model-scope");
    model_cache(&home, "claude", 100.into(), 10.into());
    for (args, expected) in [
        (vec!["run", "claude"], "work"),
        (vec!["run", "claude", "--", "--model", "sonnet"], "work"),
        (
            vec!["run", "claude", "--", "--model", "FABLE[1m]"],
            "default",
        ),
        (
            vec!["run", "claude", "--", "--model=claude-fable-5-1"],
            "default",
        ),
        (vec!["run", "claude", "--", "--model", "best"], "default"),
        (
            vec![
                "run",
                "claude",
                "--account",
                "work",
                "--",
                "--model",
                "fable",
            ],
            "work",
        ),
    ] {
        let output = command(&home, &args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with(&format!("account {expected}\n")),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = command(&home, &["accounts", "--provider", "claude", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(list["auto"], "work");
    assert_eq!(list["accounts"][2]["remaining_pct"], 90);
    model_cache(&home, "codex", 100.into(), 100.into());
    let output = command(&home, &["run", "codex", "--", "--model", "fable"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "account default\n");
}

#[test]
fn run_refuses_all_spent_fable_accounts_and_keeps_unknown_samples_usable() {
    let home = fixture("model-all-spent");
    for (fb, refused) in [
        (serde_json::json!(100), true),
        (serde_json::Value::Null, false),
        (serde_json::json!(101), false),
    ] {
        model_cache(&home, "claude", fb.clone(), fb);
        let output = command(&home, &["run", "claude", "--", "--model", "best"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            !refused,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if refused {
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("every signed-in Claude account has used its Fable limit")
            );
            assert!(!home.join("child-env").exists());
        }
    }
}
