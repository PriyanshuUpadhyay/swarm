use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "launch/account_resume.rs"]
mod account_resume;
#[path = "launch/accounts.rs"]
mod accounts;
#[path = "launch/claude_trust.rs"]
mod claude_trust;
#[path = "launch/consent.rs"]
mod consent;
#[path = "launch/listing.rs"]
mod listing;
#[path = "launch/owner.rs"]
mod owner;
#[path = "launch/providers.rs"]
mod providers;
#[path = "launch/pty_host.rs"]
mod pty_host;
#[path = "launch/resume.rs"]
mod resume;
#[path = "launch/spawning.rs"]
mod spawning;
#[path = "launch/support.rs"]
mod support;

use support::*;

const PERMISSION: &str = "────\n Bash command\n\n   touch probe.txt\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel · Tab to amend\n";
/// A chair and a seat that runs `provider`, whose screen is `$HOME/screen`, with `ring` as the
/// fake adapter's ring verb.
fn seat_session(home: &Path, provider: &str, ring: &str) -> (String, rusqlite::Connection) {
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        format!(
            "self = printf chair\nspawn = printf pane\nring = {ring}\n\
             list = printf 'pane chair\\n'\nclose = true\ncapture = cat \"$HOME/screen\"\n\
             screen = cat \"$HOME/screen\"\n"
        ),
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
    for args in [
        &["agent", "add", "orchestrator", "orchestrator"][..],
        &["spawn", "seat", "coder"],
    ] {
        let output = swarm(home, &chair(&session), args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", provider).unwrap();
    (session, connection)
}

/// `swarm send` waits for the seat's turn-start hook and stores it as the ring's proof, for each
/// provider (ADR 0041). The screen stays idle, so only the hook can prove the ring.
#[test]
fn a_send_stores_the_turn_start_hook_as_the_ring_proof() {
    for (provider, event, screen) in [
        (
            "claude",
            "UserPromptSubmit",
            include_str!("fixtures/screens/claude-idle.txt"),
        ),
        (
            "codex",
            "UserPromptSubmit",
            include_str!("fixtures/screens/codex-idle.txt"),
        ),
        (
            "agy",
            "PreInvocation",
            include_str!("fixtures/screens/agy-idle.txt"),
        ),
    ] {
        let home = scratch(&format!("hook-proof-{provider}"));
        std::fs::write(home.join("screen"), screen).unwrap();
        // The seat's own hook, as its CLI runs it when the ring starts a turn.
        let ring =
            format!("SWARM_AGENT_ID=seat \"$SWARM_EXE\" hook {provider} {event} < /dev/null");
        let (session, _connection) = seat_session(&home, provider, &ring);
        let sent = swarm(&home, &chair(&session), &["send", "seat", "task"]);
        assert!(sent.status.success(), "{provider}: {}", stderr(&sent));
        let listed = swarm(&home, &chair(&session), &["messages", "--json"]);
        let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
        assert_eq!(
            listed["messages"][0]["delivery"],
            "hook",
            "{provider}: {}",
            stderr(&sent)
        );
    }
}

/// `seat_session` whose adapter also has a `status` verb that prints `$HOME/status`, the seat's
/// program-status record, or nothing while that file is missing.
fn status_session(home: &Path, provider: &str, ring: &str) -> (String, rusqlite::Connection) {
    let made = seat_session(home, provider, ring);
    let conf = home.join(".swarm/adapters/fake.conf");
    let mut text = std::fs::read_to_string(&conf).unwrap();
    text.push_str("status = cat \"$HOME/status\" 2>/dev/null || true\n");
    std::fs::write(conf, text).unwrap();
    made
}

fn listed_seat(home: &Path, session: &str) -> serde_json::Value {
    let output = swarm(home, &chair(session), &["agents", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    list["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["id"] == "seat")
        .unwrap()
        .clone()
}

/// A pi seat's state comes from its host record, with the record's own time, ahead of the screen
/// (ADR 0070). A hooked provider keeps its hook, so a record never replaces it.
#[test]
fn a_pi_seat_shows_its_host_record_as_its_state() {
    let home = scratch("host-pi");
    std::fs::write(
        home.join("screen"),
        include_str!("fixtures/screens/pi-idle.txt"),
    )
    .unwrap();
    let (session, connection) = status_session(&home, "pi", "true");
    let status = |record: &str| std::fs::write(home.join("status"), record).unwrap();
    status("state=working:at=100");
    let seat = listed_seat(&home, &session);
    assert_eq!(
        (&seat["state"], &seat["state_source"], &seat["state_at_s"]),
        (&"working".into(), &"host".into(), &100.into()),
        "{seat}"
    );
    status("state=blocked:kind=question:at=110");
    let seat = listed_seat(&home, &session);
    assert_eq!(
        (&seat["state"], &seat["state_kind"], &seat["state_at_s"]),
        (&"waiting".into(), &"question".into(), &110.into()),
        "{seat}"
    );
    status("state=done:at=120");
    let seat = listed_seat(&home, &session);
    assert_eq!(
        (&seat["state"], &seat["state_source"], &seat["state_at_s"]),
        (&"done".into(), &"host".into(), &120.into()),
        "{seat}"
    );
    swarm::store::set_provider(&connection, &session, "seat", "claude").unwrap();
    std::fs::write(
        home.join("screen"),
        include_str!("fixtures/screens/claude-idle.txt"),
    )
    .unwrap();
    status("state=working:at=130");
    let seat = listed_seat(&home, &session);
    assert_ne!(seat["state_at_s"], 130, "{seat}");
}

/// One failed record read is no news: a fresh host state stands, as a fresh hook report does, so
/// an idle-looking screen does not end a wait and post a second wait notice. A record time ahead
/// of the clock is no record, so it cannot prove a later ring.
#[test]
fn a_failed_record_read_keeps_a_fresh_host_wait_and_a_future_record_is_none() {
    let home = scratch("host-gap");
    std::fs::write(
        home.join("screen"),
        include_str!("fixtures/screens/pi-idle.txt"),
    )
    .unwrap();
    let (session, _connection) = status_session(&home, "pi", "true");
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    };
    std::fs::write(
        home.join("status"),
        format!("state=blocked:kind=question:at={}", now()),
    )
    .unwrap();
    assert_eq!(listed_seat(&home, &session)["state"], "waiting");
    std::fs::remove_file(home.join("status")).unwrap();
    let seat = listed_seat(&home, &session);
    assert_eq!(
        (&seat["state"], &seat["state_source"]),
        (&"waiting".into(), &"host".into()),
        "{seat}"
    );
    std::fs::write(
        home.join("status"),
        format!("state=working:at={}", now() + 600),
    )
    .unwrap();
    let seat = listed_seat(&home, &session);
    assert_eq!(
        (&seat["state"], &seat["state_source"]),
        (&"waiting".into(), &"host".into()),
        "{seat}"
    );
    assert!(seat["state_at_s"].as_i64().unwrap() <= now(), "{seat}");
}

/// A pi seat has no hook, so the record its host keeps proves the ring: a turn the record shows
/// as started at or after the ring.
#[test]
fn a_host_record_after_the_ring_proves_a_pi_ring() {
    let home = scratch("host-proof-pi");
    std::fs::write(
        home.join("screen"),
        include_str!("fixtures/screens/pi-idle.txt"),
    )
    .unwrap();
    std::fs::write(home.join("status"), "state=done:at=1").unwrap();
    let (session, _connection) = status_session(
        &home,
        "pi",
        "printf 'state=working:at=%s' \"$(date +%s)\" > \"$HOME/status\"",
    );
    let sent = swarm(&home, &chair(&session), &["send", "seat", "task"]);
    assert!(sent.status.success(), "{}", stderr(&sent));
    let listed = swarm(&home, &chair(&session), &["messages", "--json"]);
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(
        listed["messages"][0]["delivery"],
        "hook",
        "{}",
        stderr(&sent)
    );
}

/// AGY draws no input box that swarm reads, so a ring to it first waits for AGY's own record,
/// which AGY reports once it takes input.
#[test]
fn a_ring_to_an_agy_seat_waits_for_its_ready_record() {
    let home = scratch("agy-ready");
    std::fs::write(
        home.join("screen"),
        include_str!("fixtures/screens/agy-idle.txt"),
    )
    .unwrap();
    let (session, _connection) = status_session(
        &home,
        "agy",
        "cat \"$HOME/status\" > \"$HOME/rang\" 2>/dev/null; SWARM_AGENT_ID=seat \"$SWARM_EXE\" hook agy PreInvocation < /dev/null",
    );
    let ready = {
        let status = home.join("status");
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            std::fs::write(status, "state=idle:at=1").unwrap();
        })
    };
    let sent = swarm(&home, &chair(&session), &["send", "seat", "task"]);
    assert!(sent.status.success(), "{}", stderr(&sent));
    ready.join().unwrap();
    assert_eq!(
        std::fs::read_to_string(home.join("rang")).unwrap_or_default(),
        "state=idle:at=1"
    );
}

/// A chair and an AGY seat whose screen is `$HOME/screen`, with one unseen message to the seat
/// whose first ring, appended to `$HOME/rings`, went out two minutes ago. Each ring starts a turn,
/// so the seat's screen shows it working and proves the ring.
fn lost_ring_session(home: &Path) -> (String, rusqlite::Connection) {
    std::fs::write(
        home.join("working"),
        include_str!("fixtures/screens/agy-working.txt"),
    )
    .unwrap();
    let (session, connection) = seat_session(
        home,
        "agy",
        "printf 'ring\\n' >> \"$HOME/rings\"; cp \"$HOME/working\" \"$HOME/screen\"",
    );
    let mut send = Command::new(env!("CARGO_BIN_EXE_swarm"));
    send.env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .envs(chair(&session))
        .args(["send", "seat", "task"])
        .stdin(std::process::Stdio::piped());
    let mut child = send.spawn().unwrap();
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
    let rings = || std::fs::read_to_string(home.join("rings")).unwrap_or_default();
    assert_eq!(rings(), "ring\n");
    // The first ring was typed a minute ago, while the CLI still started, and nothing read it.
    connection
        .execute(
            "UPDATE message SET created_at = created_at - 120, rung_at = rung_at - 120",
            [],
        )
        .unwrap();
    (session, connection)
}

fn list_agents(home: &Path, session: &str, screen: &str) {
    std::fs::write(home.join("screen"), screen).unwrap();
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(home, &chair, &["agents", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

fn rings(home: &Path) -> String {
    std::fs::read_to_string(home.join("rings")).unwrap_or_default()
}

#[test]
fn the_agent_listing_rings_again_a_message_lost_while_the_agent_started() {
    let home = scratch("rering");
    let (session, _connection) = lost_ring_session(&home);
    let list = |screen: &str| list_agents(&home, &session, screen);
    let rings = || rings(&home);

    // A pane in a turn gets no ring typed into it.
    list(include_str!("fixtures/screens/agy-working.txt"));
    assert_eq!(rings(), "ring\n");
    list(include_str!("fixtures/screens/agy-idle.txt"));
    assert_eq!(rings(), "ring\nring\n");
    // One ring again, not one per listing.
    list(include_str!("fixtures/screens/agy-idle.txt"));
    assert_eq!(rings(), "ring\nring\n");
}

#[test]
fn the_agent_listing_rings_no_agent_that_a_fresh_hook_reports_working() {
    let home = scratch("rering-hook");
    let (session, connection) = lost_ring_session(&home);
    // The hook saw the turn start a second ago, before the screen shows it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    swarm::store::set_state(
        &connection,
        &session,
        "seat",
        "working",
        "hook",
        None,
        None,
        now - 1,
    )
    .unwrap();
    list_agents(
        &home,
        &session,
        include_str!("fixtures/screens/agy-idle.txt"),
    );
    assert_eq!(rings(&home), "ring\n");
}

#[test]
fn invalid_codex_home_does_not_disable_signed_out_or_named_launch_skips() {
    let home = scratch("personal-invalid-home");
    let env = trust_session(&home);
    std::fs::create_dir_all(home.join(".codex-personal")).unwrap();
    std::fs::create_dir_all(home.join(".codex-Spare")).unwrap();
    tool(
        &home,
        "codex",
        &include_str!("fixtures/accounts/work-app-server.sh")
            .replace("*/.codex-personal)", "*/.codex-personal|*/.codex)"),
    );
    let env: Vec<_> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    for (seat, account, reason) in [
        ("personal", None, "no codex account is signed in"),
        ("spare", Some("Spare"), "no codex account named Spare"),
    ] {
        let mut args = vec!["launch", seat, "review.deep"];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        let output = swarm(&home, &env, &args);
        let text = stderr(&output);
        assert!(!output.status.success(), "{text}");
        assert!(text.contains(reason), "{text}");
        assert!(text.contains("no runner can run"), "{text}");
        assert!(!text.contains("running"), "{text}");
    }
}

#[test]
fn cached_api_key_no_source_keeps_real_auth_read_failure_fail_open() {
    let home = scratch("personal-cached-auth-read-failure");
    let env = trust_session(&home);
    std::fs::create_dir_all(home.join(".codex-personal")).unwrap();
    std::fs::create_dir_all(home.join(".codex-work")).unwrap();
    let cli = include_str!("fixtures/accounts/work-usage-app-server.sh")
        .replace("*/.codex-work)", "*/.codex-spare)");
    tool(&home, "codex", &cli);
    let refresh = swarm(
        &home,
        &[],
        &["usage", "--refresh", "--provider", "codex", "--json"],
    );
    assert!(refresh.status.success(), "{}", stderr(&refresh));
    tool(
        &home,
        "codex",
        &cli.replace(
            "echo '{\"id\":2,\"result\":{\"account\":{\"type\":\"apiKey\"}}}'",
            "exit 1",
        ),
    );
    let accounts = swarm(&home, &[], &["accounts", "--provider", "codex", "--json"]);
    assert!(accounts.status.success(), "{}", stderr(&accounts));
    let list: serde_json::Value = serde_json::from_slice(&accounts.stdout).unwrap();
    assert_eq!(list["schema"], "swarm.accounts.v1");
    let rows = list["accounts"].as_array().unwrap();
    let personal = rows.iter().find(|row| row["name"] == "personal").unwrap();
    assert_eq!(personal["auth_state"], "unavailable");
    assert_eq!(personal["usage_state"], "no_source");
    let work = rows.iter().find(|row| row["name"] == "work").unwrap();
    assert_eq!(work["auth_state"], "signed_out");
    assert!(rows.iter().all(|row| row.get("invalid_home").is_none()));
    let env: Vec<_> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let output = swarm(&home, &env, &["launch", "personal", "review.deep"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("swarm: review.deep: running codex/gpt-6-luna/high"),
        "{}",
        stderr(&output)
    );
    assert!(!stderr(&output).contains("no codex account is signed in"));
}

fn launch_in(home: &Path, env: &[(&str, String)], seat: &str, role: &str, cwd: &Path) -> Output {
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let cwd = cwd.to_string_lossy().into_owned();
    let output = swarm(home, &env, &["launch", seat, role, "--cwd", &cwd]);
    assert!(output.status.success(), "{seat}: {}", stderr(&output));
    output
}
