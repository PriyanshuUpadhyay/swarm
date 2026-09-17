use serde_json::Value;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
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

fn run_with_stdin(fixture: &Fixture, args: &[&str], input: &str) -> Output {
    let mut child = command(fixture, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn unscoped_command(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .args(args)
        .env("SWARM_HOME", home)
        .env_remove("SWARM_SESSION_ID")
        .env_remove("SWARM_AGENT_ID")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("CLAUDE_CONFIG_DIR");
    command
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
    let session = unscoped_command(&home, &["session", "new", "lane"])
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
fn session_new_records_working_directory_time_and_safe_chair_log() {
    let fixture = fixture("session-metadata");
    let config = fixture.root.join("claude");
    let project = config.join("projects/project-slug");
    std::fs::create_dir_all(&project).unwrap();
    let log = project.join("abc-123.jsonl");
    std::fs::write(&log, "transcript").unwrap();
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let output = unscoped_command(&fixture.home, &["session", "new", "relay"])
        .env("CLAUDE_CONFIG_DIR", &config)
        .env("CLAUDE_CODE_SESSION_ID", "abc-123")
        .env("SWARM_ADAPTER", "herdr")
        .current_dir(&fixture.root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let session: i64 = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let connection = swarm::store::open(&fixture.home.join(".swarm/swarm.db")).unwrap();
    let metadata: (String, i64, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT cwd, created_at, chair_log, adapter FROM session WHERE id = ?1",
            [session],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(metadata.0, fixture.root.canonicalize().unwrap().to_string_lossy());
    assert!((before..=after).contains(&metadata.1));
    assert_eq!(metadata.2.as_deref(), Some(log.to_string_lossy().as_ref()));
    assert_eq!(metadata.3.as_deref(), Some("herdr"));

    for id in [None, Some("bad/id"), Some(".."), Some("missing")] {
        let mut command = unscoped_command(&fixture.home, &["session", "new", "lane"]);
        command.env("CLAUDE_CONFIG_DIR", &config).current_dir(&fixture.root);
        if let Some(id) = id {
            command.env("CLAUDE_CODE_SESSION_ID", id);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let session: i64 = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
        assert_eq!(
            connection
                .query_row("SELECT chair_log FROM session WHERE id = ?1", [session], |row| row.get::<_, Option<String>>(0))
                .unwrap(),
            None,
            "id {id:?}"
        );
    }
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn sessions_json_excludes_legacy_rows_orders_and_counts_without_identity() {
    let fixture = fixture("sessions");
    let db = fixture.home.join(".swarm/swarm.db");
    let root = fixture.home.join(".swarm");
    let mut connection = swarm::store::open(&db).unwrap();
    connection
        .execute("INSERT INTO session (talk_mode) VALUES ('lane')", [])
        .unwrap();
    let legacy = connection.last_insert_rowid();
    let older = swarm::store::create_session(&connection, "relay", Path::new("/work/older"), None, None).unwrap();
    let newer = swarm::store::create_session(&connection, "open", Path::new("/work/newer"), None, Some("herdr")).unwrap();
    connection
        .execute("UPDATE session SET created_at = 10 WHERE id = ?1", [older])
        .unwrap();
    connection
        .execute("UPDATE session SET created_at = 20 WHERE id = ?1", [newer])
        .unwrap();
    connection
        .execute("UPDATE session SET created_at = 1 WHERE id = ?1", [fixture.session.parse::<i64>().unwrap()])
        .unwrap();
    swarm::store::add_agent(&connection, older, "older-chair", "orchestrator").unwrap();
    swarm::store::add_agent(&connection, newer, "newer-chair", "orchestrator").unwrap();
    swarm::store::add_agent(&connection, newer, "coder", "coder").unwrap();
    swarm::store::send_message(&mut connection, &root, newer, "newer-chair", "coder", "ask", "one").unwrap();
    swarm::store::send_message(&mut connection, &root, newer, "coder", "newer-chair", "summary", "two").unwrap();
    connection.execute("UPDATE message SET created_at = 30 WHERE session_id = ?1 AND sender_id = 'newer-chair'", [newer]).unwrap();
    connection.execute("UPDATE message SET created_at = 40 WHERE session_id = ?1 AND sender_id = 'coder'", [newer]).unwrap();
    drop(connection);

    let output = unscoped_command(&fixture.home, &["sessions", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let sessions = json["sessions"].as_array().unwrap();
    assert_eq!(sessions[0], serde_json::json!({
        "id": newer,
        "talk_mode": "open",
        "adapter": "herdr",
        "cwd": "/work/newer",
        "created_at": 20,
        "chair_log": null,
        "agents": 2,
        "messages": 2,
        "last_message_at": 40
    }));
    assert_eq!(sessions[1]["id"], older);
    assert_eq!(sessions[1]["adapter"], Value::Null);
    assert_eq!(sessions[1]["agents"], 1);
    assert_eq!(sessions[1]["messages"], 0);
    assert_eq!(sessions[1]["last_message_at"], Value::Null);
    assert!(!sessions.iter().any(|session| session["id"] == legacy));
    std::fs::remove_dir_all(fixture.root).unwrap();
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
fn type_runs_ring_without_writing_a_message_and_rejects_empty_text_or_missing_pane() {
    let fixture = fixture("type");
    assert!(run(&fixture, &["spawn", "coder-1", "code.complex"]).status.success());
    let ring = fixture.root.join("ring");
    adapter(
        &fixture,
        &format!(
            "self = true\nspawn = true\nring = printf '%s:%s' \"$SWARM_PANE\" \"$SWARM_TEXT\" > '{}'\nlist = true\nclose = true\ncapture = true\ninterrupt = true\n",
            ring.display()
        ),
    );

    let typed = run_with_stdin(&fixture, &["type", "coder-1"], "hello");
    assert!(typed.status.success(), "{}", String::from_utf8_lossy(&typed.stderr));
    assert!(typed.stdout.is_empty());
    assert_eq!(std::fs::read_to_string(&ring).unwrap(), "%1:hello");
    let connection = swarm::store::open(&fixture.home.join(".swarm/swarm.db")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM message", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );

    let empty = run_with_stdin(&fixture, &["type", "coder-1"], " \n\t");
    assert!(!empty.status.success());
    assert_eq!(String::from_utf8_lossy(&empty.stderr), "swarm: empty text\n");
    let missing = run_with_stdin(&fixture, &["type", "orchestrator"], "hello");
    assert!(!missing.status.success());
    assert_eq!(String::from_utf8_lossy(&missing.stderr), "swarm: no pane recorded\n");
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn interrupt_runs_optional_verb_and_reports_missing_support_or_pane() {
    let fixture = fixture("interrupt");
    assert!(run(&fixture, &["spawn", "coder-1", "code.complex"]).status.success());
    let interrupted = fixture.root.join("interrupted");
    adapter(
        &fixture,
        &format!(
            "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\ninterrupt = printf '%s' \"$SWARM_PANE\" > '{}'\n",
            interrupted.display()
        ),
    );

    let output = run(&fixture, &["interrupt", "coder-1"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read_to_string(interrupted).unwrap(), "%1");
    let missing = run(&fixture, &["interrupt", "orchestrator"]);
    assert!(!missing.status.success());
    assert_eq!(String::from_utf8_lossy(&missing.stderr), "swarm: no pane recorded\n");

    adapter(
        &fixture,
        "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
    );
    let unsupported = run(&fixture, &["interrupt", "coder-1"]);
    assert!(!unsupported.status.success());
    assert_eq!(
        String::from_utf8_lossy(&unsupported.stderr),
        "swarm: adapter fake has no interrupt\n"
    );
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
