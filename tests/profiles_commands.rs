use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("swarm-{name}-{}-{suffix}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

fn run(args: &[&str], routing: &Path, yelo: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(args)
        .env("SWARM_ROUTING_CMD", routing)
        .env("SWARM_YELO_CMD", yelo)
        .output()
        .unwrap()
}

fn fixtures(root: &Path) -> (PathBuf, PathBuf) {
    let routing = root.join("routing");
    script(
        &routing,
        r#"test "$*" = "web-state"
printf '%s' '{"path":"/fixture/roles.json","config":{"routes":{"ORCHESTRATOR":["claudeLead"],"CODER":["codexWork","claudeBackup"]},"runners":{"claudeLead":{"provider":"claude","model":"opus","effort":"xhigh"},"codexWork":{"provider":"codex","model":"gpt-work","effort":"high","sandbox":"workspace-write"},"claudeBackup":{"provider":"claude","model":"sonnet"}}}}'"#,
    );
    let yelo = root.join("yelo");
    script(
        &yelo,
        r#"case "$*" in
  "profile list --cli claude --usage --json")
    printf '%s' '[{"name":"claudeWorkAccount","dir":"/profiles/claude","email":"claude@example.com","signed_in":true,"remaining":52,"usage":"7d 52% left"}]' ;;
  "profile pick --cli claude --json")
    printf '%s' '{"name":"claudeWorkAccount","dir":"/profiles/claude"}' ;;
  "profile list --cli codex --usage --json")
    printf '%s' '[{"name":"codexWorkAccount","dir":"/profiles/codex work","email":"codex@example.com","signed_in":true,"remaining":80,"usage":"5h 80% left"}]' ;;
  "profile pick --cli codex --json")
    printf '%s' '{"name":"codexWorkAccount","dir":"/profiles/codex work"}' ;;
  "usage show --json")
    printf '%s' '[{"label":"cl·claude@example.com","provider":"claude","window":"7d","pct":48,"reset":"4d22h","state":"ok","asOf":1789576942},{"label":"cx·other@example.com","provider":"codex","window":"5h","pct":20,"reset":null,"state":"stale","asOf":null}]' ;;
  *) echo "unexpected yelo call: $*" >&2; exit 9 ;;
esac"#,
    );
    (routing, yelo)
}

#[test]
fn json_commands_translate_fake_tool_output() {
    let root = temp_dir("profile-commands");
    let (routing, yelo) = fixtures(&root);

    let roles = run(&["roles", "--json"], &routing, &yelo);
    assert!(
        roles.status.success(),
        "{}",
        String::from_utf8_lossy(&roles.stderr)
    );
    let roles: Value = serde_json::from_slice(&roles.stdout).unwrap();
    assert_eq!(roles["roles"][0]["role"], "CODER");
    assert_eq!(
        roles["roles"][0]["fallbacks"],
        serde_json::json!(["claudeBackup"])
    );

    let accounts = run(
        &["accounts", "--provider", "codex", "--json"],
        &routing,
        &yelo,
    );
    assert!(
        accounts.status.success(),
        "{}",
        String::from_utf8_lossy(&accounts.stderr)
    );
    let accounts: Value = serde_json::from_slice(&accounts.stdout).unwrap();
    assert_eq!(accounts["auto"], "codexWorkAccount");
    assert_eq!(
        accounts["accounts"][0]["env"]["CODEX_HOME"],
        "/profiles/codex work"
    );

    let agy = run(
        &["accounts", "--provider", "agy", "--json"],
        &routing,
        &yelo,
    );
    assert!(
        agy.status.success(),
        "{}",
        String::from_utf8_lossy(&agy.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&agy.stdout).unwrap(),
        serde_json::json!({"provider":"agy","source":null,"accounts":[],"auto":null})
    );

    let usage = run(&["usage", "--json"], &routing, &yelo);
    assert!(
        usage.status.success(),
        "{}",
        String::from_utf8_lossy(&usage.stderr)
    );
    let usage: Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert_eq!(usage["meters"][0]["account"], "claudeWorkAccount");
    assert_eq!(usage["meters"][1]["account"], Value::Null);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn spawn_quotes_selected_account_and_rejects_unknown_before_spawn() {
    let root = temp_dir("profile-spawn");
    let (routing, yelo) = fixtures(&root);
    let swarm_home = root.join("home");
    let opened = root.join("opened");
    let ring = root.join("ring");

    let init = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("init")
        .env("SWARM_HOME", &swarm_home)
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    std::fs::write(
        swarm_home.join(".swarm/adapters/fake.conf"),
        format!(
            "self = printf self\nspawn = printf opened >> '{}'; printf pane-1\nring = printf '%s' \"$SWARM_TEXT\" > '{}'\nlist = true\nclose = true\ncapture = true\n",
            opened.display(),
            ring.display()
        ),
    )
    .unwrap();
    let session = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["session", "new", "lane"])
        .env("SWARM_HOME", &swarm_home)
        .output()
        .unwrap();
    assert!(
        session.status.success(),
        "{}",
        String::from_utf8_lossy(&session.stderr)
    );
    let session = String::from_utf8(session.stdout).unwrap();

    let spawned = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "spawn",
            "worker",
            "CODER",
            "--account",
            "auto",
            "--",
            "printf",
            "%s",
            "hello world",
        ])
        .env("SWARM_HOME", &swarm_home)
        .env("SWARM_SESSION_ID", session.trim())
        .env("SWARM_ADAPTER", "fake")
        .env("SWARM_ROUTING_CMD", &routing)
        .env("SWARM_YELO_CMD", &yelo)
        .output()
        .unwrap();
    assert!(
        spawned.status.success(),
        "{}",
        String::from_utf8_lossy(&spawned.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&spawned.stdout), "pane-1\n");
    assert_eq!(
        String::from_utf8_lossy(&spawned.stderr),
        "account codexWorkAccount\n"
    );
    let line = std::fs::read_to_string(&ring).unwrap();
    assert!(
        line.starts_with(
            "'env' '--' 'CODEX_HOME=/profiles/codex work' 'printf' '%s' 'hello world'; "
        )
    );
    assert_eq!(std::fs::read_to_string(&opened).unwrap(), "opened");

    let explicit = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "spawn",
            "worker2",
            "CODER",
            "--provider",
            "claude",
            "--account",
            "claudeWorkAccount",
            "--",
            "true",
        ])
        .env("SWARM_HOME", &swarm_home)
        .env("SWARM_SESSION_ID", session.trim())
        .env("SWARM_ADAPTER", "fake")
        .env("SWARM_ROUTING_CMD", &routing)
        .env("SWARM_YELO_CMD", &yelo)
        .output()
        .unwrap();
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&explicit.stderr),
        "account claudeWorkAccount\n"
    );
    assert!(
        std::fs::read_to_string(&ring)
            .unwrap()
            .starts_with("'env' '--' 'CLAUDE_CONFIG_DIR=/profiles/claude' 'true'; ")
    );

    let rejected = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["spawn", "worker3", "CODER", "--account", "missing"])
        .env("SWARM_HOME", &swarm_home)
        .env("SWARM_SESSION_ID", session.trim())
        .env("SWARM_ADAPTER", "fake")
        .env("SWARM_ROUTING_CMD", &routing)
        .env("SWARM_YELO_CMD", &yelo)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert_eq!(
        String::from_utf8_lossy(&rejected.stderr),
        "swarm: unknown codex account missing\n"
    );
    assert_eq!(std::fs::read_to_string(opened).unwrap(), "openedopened");

    std::fs::remove_dir_all(root).unwrap();
}
