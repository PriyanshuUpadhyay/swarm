use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    session: String,
    routing: PathBuf,
    yelo: PathBuf,
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

fn command(fixture: &Fixture, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .args(args)
        .env("SWARM_HOME", &fixture.home)
        .env("SWARM_SESSION_ID", &fixture.session)
        .env("SWARM_AGENT_ID", "orchestrator")
        .env("SWARM_ADAPTER", "fake")
        .env("SWARM_ROUTING_CMD", &fixture.routing)
        .env("SWARM_YELO_CMD", &fixture.yelo);
    command
}

fn run(fixture: &Fixture, args: &[&str]) -> Output {
    command(fixture, args).output().unwrap()
}

fn adapter(fixture: &Fixture, text: &str) {
    std::fs::write(fixture.home.join(".swarm/adapters/fake.conf"), text).unwrap();
}

fn tmux_command(fixture: &Fixture, args: &[&str], bin: &Path, log: &Path, mode: &str) -> Output {
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    command(fixture, args)
        .env("SWARM_ADAPTER", "tmux-solo")
        .env("FAKE_TMUX_LOG", log)
        .env("FAKE_TMUX_MODE", mode)
        .env("PATH", path)
        .output()
        .unwrap()
}

fn fake_tmux(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let bin = fixture.root.join("bin");
    let log = fixture.root.join("tmux.log");
    std::fs::create_dir_all(&bin).unwrap();
    script(
        &bin.join("tmux"),
        r#"printf '%s\n' "$*" >> "$FAKE_TMUX_LOG"
case "$FAKE_TMUX_MODE" in
  lookup-empty) printf '\n' ;;
  no-server) echo 'no server running on test socket' >&2; exit 1 ;;
  mismatch) echo 'protocol version mismatch' >&2; exit 1 ;;
  *) echo "bad fake tmux mode: $FAKE_TMUX_MODE" >&2; exit 9 ;;
esac"#,
    );
    (bin, log)
}

fn fixture(name: &str) -> Fixture {
    let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("swarm-bus-{name}-{}-{suffix}", std::process::id()));
    let home = root.join("home");
    let routing = root.join("routing");
    let yelo = root.join("yelo");
    std::fs::create_dir_all(&root).unwrap();
    script(
        &routing,
        r#"test "$1" = get
printf '%s' '{"provider":"codex","model":"gpt-test","effort":"high","sandbox":"workspace-write","approval":"never"}'"#,
    );
    script(
        &yelo,
        r#"case "$*" in
  "profile list --cli codex --usage --json")
    printf '%s' '[{"name":"work","dir":"/profiles/work","email":null,"signed_in":true,"remaining":80,"usage":"5h 80% left"}]' ;;
  "profile pick --cli codex --json") printf '%s' '{"name":"work"}' ;;
  *) exit 9 ;;
esac"#,
    );
    let init = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("init")
        .env("SWARM_HOME", &home)
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(home.join(".swarm/adapters/tmux-solo.conf").is_file());
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = true\nspawn = printf '%s' '%1'\nring = true\nlist = printf '%s' '%1'\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let session = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["session", "new", "lane"])
        .env("SWARM_HOME", &home)
        .output()
        .unwrap();
    assert!(
        session.status.success(),
        "{}",
        String::from_utf8_lossy(&session.stderr)
    );
    let fixture = Fixture {
        root,
        home,
        session: String::from_utf8(session.stdout)
            .unwrap()
            .trim()
            .to_string(),
        routing,
        yelo,
    };
    assert!(
        run(&fixture, &["agent", "add", "orchestrator", "orchestrator"])
            .status
            .success()
    );
    fixture
}

#[test]
fn agents_json_reports_live_dead_no_pane_and_list_failure() {
    let fixture = fixture("agents");
    assert!(
        run(&fixture, &["spawn", "coder-1", "code.complex"])
            .status
            .success()
    );
    adapter(
        &fixture,
        "self = true\nspawn = printf '%s' '%2'\nring = true\nlist = printf '%s' '%1'\nclose = true\ncapture = true\n",
    );
    assert!(
        run(&fixture, &["spawn", "reviewer", "review"])
            .status
            .success()
    );

    let output = run(&fixture, &["agents", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["agents"][0],
        serde_json::json!({"id":"coder-1","role":"code.complex","pane":"%1","alive":true})
    );
    assert_eq!(
        json["agents"][1],
        serde_json::json!({"id":"orchestrator","role":"orchestrator","pane":null,"alive":null})
    );
    assert_eq!(
        json["agents"][2],
        serde_json::json!({"id":"reviewer","role":"review","pane":"%2","alive":false})
    );

    adapter(
        &fixture,
        "self = true\nspawn = true\nring = true\nlist = echo broken >&2; exit 7\nclose = true\ncapture = true\n",
    );
    let failed = run(&fixture, &["agents", "--json"]);
    assert!(failed.status.success());
    assert_eq!(
        String::from_utf8_lossy(&failed.stderr),
        "swarm: list failed: broken\n"
    );
    let json: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(json["agents"][0]["alive"], Value::Null);
    assert_eq!(json["agents"][2]["alive"], Value::Null);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn messages_json_pages_reads_bodies_and_caps_at_500() {
    let fixture = fixture("messages");
    assert!(
        run(&fixture, &["agent", "add", "coder-1", "code.complex"])
            .status
            .success()
    );
    let db = fixture.home.join(".swarm/swarm.db");
    let root = fixture.home.join(".swarm");
    let mut connection = swarm::store::open(&db).unwrap();
    let session: i64 = fixture.session.parse().unwrap();
    for index in 0..502 {
        swarm::store::send_message(
            &mut connection,
            &root,
            session,
            "orchestrator",
            "coder-1",
            "ask",
            &format!("message {index}"),
        )
        .unwrap();
    }
    swarm::store::ack(&connection, session, 2, "coder-1").unwrap();
    std::fs::remove_file(root.join("runs").join(&fixture.session).join("3.txt")).unwrap();

    let output = run(&fixture, &["messages", "--json", "--after", "1"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let messages = json["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 500);
    assert_eq!(messages[0]["seq"], 2);
    assert_eq!(messages[0]["read"], true);
    assert_eq!(messages[1]["body"], Value::Null);
    assert_eq!(messages[1]["read"], false);
    assert_eq!(messages[499]["seq"], 501);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn launch_validates_resolution_and_sends_argv_to_the_pane() {
    let fixture = fixture("launch");
    let opened = fixture.root.join("opened");
    let ring = fixture.root.join("ring");
    adapter(
        &fixture,
        &format!(
            "self = true\nspawn = printf opened > '{}'; printf pane-7\nring = printf '%s' \"$SWARM_TEXT\" > '{}'\nlist = true\nclose = true\ncapture = true\n",
            opened.display(),
            ring.display()
        ),
    );

    let bad = run(&fixture, &["launch", "Bad_id", "code.complex"]);
    assert!(!bad.status.success());
    assert_eq!(
        String::from_utf8_lossy(&bad.stderr),
        "swarm: bad agent id Bad_id\n"
    );
    assert!(!opened.exists());

    script(&fixture.routing, "echo route failed >&2; exit 4");
    let failed = run(&fixture, &["launch", "coder-1", "code.complex"]);
    assert_eq!(
        String::from_utf8_lossy(&failed.stderr),
        "swarm: cannot resolve role code.complex: route failed\n"
    );
    assert!(!opened.exists());

    script(
        &fixture.routing,
        "printf '%s' '{\"error\":\"unknown route\"}'",
    );
    let errored = run(&fixture, &["launch", "coder-1", "code.complex"]);
    assert_eq!(
        String::from_utf8_lossy(&errored.stderr),
        "swarm: cannot resolve role code.complex: unknown route\n"
    );
    assert!(!opened.exists());

    script(
        &fixture.routing,
        r#"printf '%s' '{"provider":"codex","model":"gpt-test","effort":"high","sandbox":"workspace-write","approval":"never"}'"#,
    );
    let launched = run(
        &fixture,
        &["launch", "coder-1", "code.complex", "--account", "auto"],
    );
    assert!(
        launched.status.success(),
        "{}",
        String::from_utf8_lossy(&launched.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launched.stdout), "pane-7\n");
    assert_eq!(String::from_utf8_lossy(&launched.stderr), "account work\n");
    let line = std::fs::read_to_string(ring).unwrap();
    assert!(line.contains("'CODEX_HOME=/profiles/work' 'codex' '--model' 'gpt-test'"));
    assert!(line.contains("'sandbox_workspace_write.writable_roots=[\""));
    assert!(line.contains("/.swarm\"]' '--ask-for-approval' 'never'"));
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn attach_reports_missing_support_or_pane_and_passes_exit_status() {
    let fixture = fixture("attach");
    assert!(
        run(&fixture, &["spawn", "coder-1", "code.complex"])
            .status
            .success()
    );
    let unsupported = run(&fixture, &["attach", "coder-1"]);
    assert!(!unsupported.status.success());
    assert_eq!(
        String::from_utf8_lossy(&unsupported.stderr),
        "swarm: adapter fake has no attach\n"
    );

    adapter(
        &fixture,
        "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\nattach = exit 7\n",
    );
    let missing = run(&fixture, &["attach", "orchestrator"]);
    assert!(!missing.status.success());
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        "swarm: no pane recorded\n"
    );
    assert_eq!(run(&fixture, &["attach", "coder-1"]).status.code(), Some(7));
    adapter(
        &fixture,
        "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\nattach = kill -TERM $$\n",
    );
    assert_eq!(run(&fixture, &["attach", "coder-1"]).status.code(), Some(143));
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn tmux_solo_rejects_an_empty_session_lookup_for_close_and_attach() {
    let fixture = fixture("tmux-empty-session");
    assert!(run(&fixture, &["spawn", "coder-1", "code.complex"]).status.success());
    let (bin, log) = fake_tmux(&fixture);

    let closed = tmux_command(&fixture, &["close", "coder-1"], &bin, &log, "lookup-empty");
    assert!(!closed.status.success());
    assert_eq!(String::from_utf8_lossy(&closed.stderr), "close failed: no session for %1\n");
    let connection = swarm::store::open(&fixture.home.join(".swarm/swarm.db")).unwrap();
    assert_eq!(
        swarm::store::pane_of(&connection, fixture.session.parse().unwrap(), "coder-1")
            .unwrap()
            .as_deref(),
        Some("%1")
    );

    let attached = tmux_command(&fixture, &["attach", "coder-1"], &bin, &log, "lookup-empty");
    assert!(!attached.status.success());
    assert_eq!(String::from_utf8_lossy(&attached.stderr), "no session for %1\n");
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(!calls.contains("kill-session"));
    assert!(!calls.contains("attach-session"));
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn tmux_solo_list_ignores_only_no_server_errors() {
    let fixture = fixture("tmux-list-errors");
    assert!(run(&fixture, &["spawn", "coder-1", "code.complex"]).status.success());
    let (bin, log) = fake_tmux(&fixture);

    let missing = tmux_command(&fixture, &["agents", "--json"], &bin, &log, "no-server");
    assert!(missing.status.success());
    assert!(missing.stderr.is_empty());
    let json: Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(json["agents"][0]["alive"], false);

    let failed = tmux_command(&fixture, &["agents", "--json"], &bin, &log, "mismatch");
    assert!(failed.status.success());
    assert_eq!(String::from_utf8_lossy(&failed.stderr), "swarm: list failed: protocol version mismatch\n");
    let json: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(json["agents"][0]["alive"], Value::Null);

    let swept = tmux_command(&fixture, &["sweep"], &bin, &log, "mismatch");
    assert!(!swept.status.success());
    let connection = swarm::store::open(&fixture.home.join(".swarm/swarm.db")).unwrap();
    assert_eq!(
        swarm::store::pane_of(&connection, fixture.session.parse().unwrap(), "coder-1")
            .unwrap()
            .as_deref(),
        Some("%1")
    );
    std::fs::remove_dir_all(fixture.root).unwrap();
}
