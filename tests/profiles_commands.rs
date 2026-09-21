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
    echo 'claude: no usage data to pick an account from' >&2; exit 1 ;;
  "profile list --cli codex --usage --json")
    printf '%s' '[{"name":"codexWorkAccount","dir":"/profiles/codex work","email":"codex@example.com","signed_in":true,"remaining":80,"usage":"5h 80% left"},{"name":"codexNoEmailAccount","dir":"/profiles/codex-no-email","email":null,"signed_in":false,"remaining":null,"usage":null},{"name":null,"dir":"/profiles/codex-nameless","email":null,"signed_in":true,"remaining":90,"usage":"7d 90% left"}]' ;;
  "profile pick --cli codex --json")
    printf '%s' '{"name":"codexWorkAccount","dir":"/profiles/codex work"}' ;;
  "usage show --json")
    printf '%s' '[{"label":"cl·claude@example.com","provider":"claude","window":"7d","pct":48,"reset":"4d22h","state":"ok","asOf":1789576942},{"label":"cx","provider":"codex","state":"logged_out","reason":"logged out"},{"label":"cx·codexNoEmailAccount","provider":"codex","state":"missing","reason":"no data"},{"provider":"codex"}]' ;;
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
    assert_eq!(accounts["accounts"].as_array().unwrap().len(), 2);
    assert_eq!(
        accounts["accounts"][0]["env"]["CODEX_HOME"],
        "/profiles/codex work"
    );

    let failed_pick = run(
        &["accounts", "--provider", "claude", "--json"],
        &routing,
        &yelo,
    );
    assert!(
        failed_pick.status.success(),
        "{}",
        String::from_utf8_lossy(&failed_pick.stderr)
    );
    let failed_pick: Value = serde_json::from_slice(&failed_pick.stdout).unwrap();
    assert_eq!(failed_pick["auto"], Value::Null);
    assert_eq!(failed_pick["accounts"][0]["name"], "claudeWorkAccount");

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
    assert!(
        String::from_utf8_lossy(&usage.stderr)
            .starts_with("swarm: skipped usage row: missing field `label`\n")
    );
    let usage: Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert_eq!(usage["meters"][0]["account"], "claudeWorkAccount");
    assert_eq!(usage["meters"][1]["account"], Value::Null);
    assert_eq!(usage["meters"][1]["window"], Value::Null);
    assert_eq!(usage["meters"][1]["used_pct"], Value::Null);
    assert_eq!(usage["meters"][1]["reason"], "logged out");
    assert_eq!(usage["meters"][2]["account"], "codexNoEmailAccount");

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
    // Claude keeps its credentials outside the config dir, so a seat that gets only
    // CLAUDE_CONFIG_DIR starts at "Not logged in".
    assert!(std::fs::read_to_string(&ring).unwrap().starts_with(&format!(
        "'env' '--' 'AGENT_PROFILE_LABEL=claudeWorkAccount' \
         'CLAUDE_CONFIG_DIR=/profiles/claude' \
         'CLAUDE_SECURESTORAGE_CONFIG_DIR={}/.claude-claudeWorkAccount' 'true'; ",
        swarm_home.display()
    )));

    let ignored_provider = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "spawn",
            "worker3",
            "CODER",
            "--provider",
            "codex",
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
    assert!(!ignored_provider.status.success());
    assert!(String::from_utf8_lossy(&ignored_provider.stderr).starts_with("usage: swarm"));

    let rejected = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["spawn", "worker4", "CODER", "--account", "missing"])
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

#[test]
fn account_rides_the_command_not_the_session() {
    let root = temp_dir("profile-account-env");
    let (routing, yelo) = fixtures(&root);
    let swarm_home = root.join("home");
    let spawn_env_log = root.join("spawn-env");
    let ring_log = root.join("ring");

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
            "self = printf self\nspawn = env > '{}'; printf pane-1\nring = printf '%s' \"$SWARM_TEXT\" > '{}'\nlist = true\nclose = true\ncapture = true\n",
            spawn_env_log.display(),
            ring_log.display()
        ),
    )
    .unwrap();
    let session = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["session", "new", "lane"])
        .env("SWARM_HOME", &swarm_home)
        .output()
        .unwrap();
    assert!(session.status.success());
    let session = String::from_utf8(session.stdout).unwrap();

    let spawned = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "spawn",
            "worker",
            "CODER",
            "--account",
            "codexWorkAccount",
            "--",
            "printf",
            "%s",
            "hello",
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

    let command_line = std::fs::read_to_string(&ring_log).unwrap();
    assert!(
        command_line.starts_with(
            "'env' '--' 'CODEX_HOME=/profiles/codex work' 'printf' '%s' 'hello'; "
        ),
        "expected command to carry account vars with env --, got: {command_line}"
    );

    let spawn_env = std::fs::read_to_string(&spawn_env_log).unwrap();
    assert!(
        !spawn_env.lines().any(|line| line.starts_with("CODEX_HOME=")),
        "spawn session environment should not contain account variables"
    );

    std::fs::remove_dir_all(root).unwrap();
}
