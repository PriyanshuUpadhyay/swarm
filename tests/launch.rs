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

/// Runs the built binary with only HOME, PATH, and the given variables set. SWARM_HOME pins the
/// data to HOME, so a branch build (ADR 0027) reads the same place as a `main` one.
fn swarm(home: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
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
        &["launch", "seat", "review.deep"],
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

fn trusted(config: &Path, dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(config) else {
        return false;
    };
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["projects"][dir.to_string_lossy().as_ref()]["hasTrustDialogAccepted"] == true
}

#[test]
fn a_claude_launch_trusts_the_config_that_the_pane_reads() {
    let home = scratch("trust");
    let profiles = ["a", "b"].map(|name| home.join(".claude/.profiles").join(name));
    for dir in &profiles {
        std::fs::create_dir_all(dir).unwrap();
    }
    // yelo keeps its own data next to the profiles, in hidden dirs that no pane reads.
    let hidden = home.join(".claude/.profiles/.session-map");
    std::fs::create_dir_all(&hidden).unwrap();
    let rows: Vec<_> = profiles
        .iter()
        .zip(["a", "b"])
        .map(|(dir, name)| {
            serde_json::json!({"name": name, "dir": dir, "signed_in": true, "remaining": 50})
        })
        .collect();
    let list = serde_json::Value::from(rows);
    tool(
        &home,
        "yelo",
        &format!("case \"$*\" in *pick*) echo '{{\"name\":\"a\"}}' ;; *) echo '{list}' ;; esac"),
    );
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"review.deep": ["claude-opus"]},
            "runners": {"claude-opus": {"provider": "claude", "model": "opus", "effort": "high"}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(&home, &fake, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let launch = |seat: &str, repo: &str, account: Option<&str>| {
        let cwd = home.join(repo);
        std::fs::create_dir_all(&cwd).unwrap();
        let cwd = cwd.to_string_lossy().into_owned();
        let mut args = vec!["launch", seat, "review.deep", "--cwd", &cwd];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        let output = swarm(&home, &env, &args);
        assert!(output.status.success(), "{seat}: {}", stderr(&output));
        home.join(repo).join(".herdr/workers")
    };

    // No --account: yelo's `claude` in the pane picks any profile, or ~/.claude.json without yelo.
    let any = launch("seat-any", "any", None);
    assert!(trusted(&home.join(".claude.json"), &any));
    assert!(!hidden.join(".claude.json").exists());
    for dir in &profiles {
        assert!(
            trusted(&dir.join(".claude.json"), &any),
            "{}",
            dir.display()
        );
    }

    // --account b: the pane gets b's CLAUDE_CONFIG_DIR, so only b's config needs the entry.
    let only_b = launch("seat-b", "only-b", Some("b"));
    assert!(trusted(&profiles[1].join(".claude.json"), &only_b));
    assert!(!trusted(&profiles[0].join(".claude.json"), &only_b));
    assert!(!trusted(&home.join(".claude.json"), &only_b));

    // --account auto: yelo picks a, then b on every later call, as when usage moves in between.
    // The pane must get the account whose config got the entry.
    tool(
        &home,
        "yelo",
        &format!(
            "case \"$*\" in *pick*) if [ -e \"$HOME/picked\" ]; then echo '{{\"name\":\"b\"}}'; \
             else touch \"$HOME/picked\"; echo '{{\"name\":\"a\"}}'; fi ;; *) echo '{list}' ;; esac"
        ),
    );
    let cwd = home.join("auto");
    std::fs::create_dir_all(&cwd).unwrap();
    let output = swarm(
        &home,
        &env,
        &[
            "launch",
            "seat-auto",
            "review.deep",
            "--cwd",
            &cwd.to_string_lossy(),
            "--account",
            "auto",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("account a"), "{}", stderr(&output));
    assert!(trusted(
        &profiles[0].join(".claude.json"),
        &cwd.join(".herdr/workers")
    ));

    // A checked-out repo can commit `.herdr` or `.herdr/workers` as a link, and trusting where
    // it points could trust any folder, such as `/`.
    let target = home.join("target");
    std::fs::create_dir_all(target.join("workers")).unwrap();
    for (repo, link, points_to) in [
        ("link-herdr", ".herdr", target.clone()),
        ("link-workers", ".herdr/workers", target.join("workers")),
    ] {
        let cwd = home.join(repo);
        std::fs::create_dir_all(cwd.join(link).parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&points_to, cwd.join(link)).unwrap();
        let cwd = cwd.to_string_lossy().into_owned();
        let output = swarm(&home, &env, &["launch", repo, "review.deep", "--cwd", &cwd]);
        assert!(!output.status.success(), "{repo} launched");
        assert!(
            stderr(&output).contains("symlink"),
            "{repo}: {}",
            stderr(&output)
        );
        assert!(
            !trusted(&home.join(".claude.json"), &target.join("workers")),
            "{repo}"
        );
    }

    // A resuming child runs in cwd itself, so a linked `.herdr` below it does not matter.
    let resumed = home.join("link-herdr");
    let cwd = resumed.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &[
            "launch",
            "seat-resume",
            "review.deep",
            "--cwd",
            &cwd,
            "--",
            "--resume",
            "x",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(trusted(&home.join(".claude.json"), &resumed));

    // The app hides the chair's pane, so nobody answers a trust dialog there; it runs in cwd.
    let chair = launch("orchestrator", "chair", None);
    assert!(trusted(&home.join(".claude.json"), &home.join("chair")));
    assert!(trusted(
        &profiles[0].join(".claude.json"),
        &home.join("chair")
    ));
    assert!(!chair.exists());
}

/// A session with a chair and one child on a fake adapter whose screen is `$HOME/screen` and
/// whose `key` verb appends to `$HOME/keys`. Its history is the same screen. `key_then` runs after each key, to change the screen.
fn answer_session(home: &Path, key_then: &str) -> String {
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        format!(
            "self = printf chair\nspawn = printf pane\nring = true\nlist = printf 'pane claude\\n'\n\
             close = true\ncapture = cat \"$HOME/screen\"\nscreen = cat \"$HOME/screen\"\n\
             key = printf '%s\\n' \"$SWARM_KEY\" >> \"$HOME/keys\"; {key_then}\n"
        ),
    )
    .unwrap();
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(home, &fake, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    for args in [
        &["agent", "add", "orchestrator", "orchestrator"][..],
        &["spawn", "seat", "coder", "--", "true"],
    ] {
        let output = swarm(home, &chair, args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }
    // The screen check reads only an agent with a provider; `spawn --provider` needs an account.
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "claude").unwrap();
    session
}

fn listed_prompt(home: &Path, session: &str) -> serde_json::Value {
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(home, &chair, &["agents", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    list["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["id"] == "seat")
        .unwrap()["prompt"]
        .clone()
}

const PERMISSION: &str = "────\n Bash command\n\n   touch probe.txt\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel · Tab to amend\n";
const ARROWS: &str =
    "● Checking the folder.\n Do you want to proceed?\n > Yes\n   No\n tab amend\n";
const FOLDER_TRUST: &str =
    "  Trust this folder?\n\n› 1. Trust and continue\n  2. Back\n\n  enter continue · esc back\n";

#[test]
fn the_owner_answers_a_listed_question_with_its_choice_key() {
    let home = scratch("answer");
    let session = answer_session(&home, "printf 'done\\n' > \"$HOME/screen\"");
    std::fs::write(home.join("screen"), PERMISSION).unwrap();
    let prompt = listed_prompt(&home, &session);
    assert_eq!(prompt["choices"], serde_json::json!(["Yes", "No"]));
    let id = prompt["id"].as_str().unwrap().to_string();
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let keys = || std::fs::read_to_string(home.join("keys")).unwrap_or_default();

    // Inside an agent pane nobody answers for the owner.
    let mut in_pane = app.to_vec();
    in_pane.push(("TMUX_PANE", "%9"));
    let output = swarm(&home, &in_pane, &["answer", "seat", &id, "1"]);
    assert!(
        stderr(&output).contains("only the owner answers"),
        "{}",
        stderr(&output)
    );
    // A stale id and a missing choice send nothing.
    let output = swarm(&home, &app, &["answer", "seat", "0000000000000000", "1"]);
    assert!(
        stderr(&output).contains("the question changed"),
        "{}",
        stderr(&output)
    );
    let output = swarm(&home, &app, &["answer", "seat", &id, "2"]);
    assert!(
        stderr(&output).contains("no choice 2"),
        "{}",
        stderr(&output)
    );
    assert_eq!(keys(), "");

    // The digit picks the choice; the screen moves on, so no Enter follows.
    let output = swarm(&home, &app, &["answer", "seat", &id, "1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(keys(), "2\n");
    assert_eq!(listed_prompt(&home, &session), serde_json::Value::Null);
}

#[test]
fn an_arrow_answer_sends_enter_only_once_the_cursor_moved() {
    let home = scratch("answer-arrows");
    let session = answer_session(&home, "cp \"$HOME/moved\" \"$HOME/screen\"");
    std::fs::write(home.join("screen"), ARROWS).unwrap();
    std::fs::write(
        home.join("moved"),
        ARROWS.replace(" > Yes\n   No", "   Yes\n > No"),
    )
    .unwrap();
    let id = listed_prompt(&home, &session)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(&home, &app, &["answer", "seat", &id, "1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(home.join("keys")).unwrap(),
        "Down\nEnter\n"
    );

    // A cursor left on another choice means the screen took some other key: no Enter.
    std::fs::remove_file(home.join("keys")).unwrap();
    std::fs::write(home.join("screen"), ARROWS).unwrap();
    std::fs::write(home.join("moved"), ARROWS).unwrap();
    let output = swarm(&home, &app, &["answer", "seat", &id, "1"]);
    assert!(
        stderr(&output).contains("changed while the answer was sent"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        std::fs::read_to_string(home.join("keys")).unwrap(),
        "Down\n"
    );
}

#[test]
fn a_folder_trust_screen_that_nothing_closes_above_is_answered_only_in_the_pane() {
    let home = scratch("answer-trust");
    let session = answer_session(&home, "true");
    std::fs::write(home.join("screen"), FOLDER_TRUST).unwrap();
    assert_eq!(listed_prompt(&home, &session), serde_json::Value::Null);
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(&home, &app, &["answer", "seat", "0000000000000000", "0"]);
    assert!(
        stderr(&output).contains("shows no question now"),
        "{}",
        stderr(&output)
    );
    assert!(!home.join("keys").exists());
}

#[test]
fn a_second_answer_to_the_same_pane_is_refused_and_sends_nothing() {
    let home = scratch("answer-lock");
    let session = answer_session(&home, "true");
    std::fs::write(home.join("screen"), PERMISSION).unwrap();
    let id = listed_prompt(&home, &session)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    // Another answer to this pane holds its lock.
    let held = std::fs::File::create(home.join(".swarm/answer-pane.lock")).unwrap();
    held.lock().unwrap();
    let output = swarm(&home, &app, &["answer", "seat", &id, "0"]);
    assert!(
        stderr(&output).contains("still being sent"),
        "{}",
        stderr(&output)
    );
    assert!(!home.join("keys").exists());
    drop(held);
    let output = swarm(&home, &app, &["answer", "seat", &id, "0"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(std::fs::read_to_string(home.join("keys")).unwrap(), "1\n");
}

#[test]
fn a_child_agent_can_neither_launch_nor_spawn() {
    let home = scratch("child");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = touch \"$HOME/spawned\"; printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
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
    let child = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "cl-seat-1"),
    ];
    for args in [
        &["launch", "grandchild", "review.deep"][..],
        &["spawn", "grandchild", "coder", "--", "true"],
    ] {
        let output = swarm(&home, &child, args);
        assert!(!output.status.success(), "{args:?} passed");
        assert!(
            stderr(&output).contains("a child agent cannot launch agents"),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    assert!(!home.join("spawned").exists());
}

/// A chair and an AGY seat whose screen is `$HOME/screen`, with one unseen message to the seat
/// whose first ring, appended to `$HOME/rings`, went out two minutes ago.
fn lost_ring_session(home: &Path) -> (String, rusqlite::Connection) {
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = printf 'ring\\n' >> \"$HOME/rings\"\n\
         list = printf 'pane chair\\n'\nclose = true\ncapture = cat \"$HOME/screen\"\n\
         screen = cat \"$HOME/screen\"\n",
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
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    for args in [
        &["agent", "add", "orchestrator", "orchestrator"][..],
        &["spawn", "seat", "coder"],
    ] {
        let output = swarm(home, &chair, args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "agy").unwrap();
    let mut send = Command::new(env!("CARGO_BIN_EXE_swarm"));
    send.env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .envs(chair)
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
fn a_spawned_agent_runs_the_swarm_that_launched_it() {
    let home = scratch("own-swarm");
    // An older `swarm` installed first on the pane's PATH.
    tool(&home, "swarm", "exit 1");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
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
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(
        &home,
        &chair,
        &[
            "spawn",
            "seat",
            "coder",
            "--",
            "sh",
            "-c",
            "command -v swarm > \"$HOME/which\"",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    // The pane's shell sources the run script with its own PATH.
    let script = home.join(format!(".swarm/runs/{session}/seat.sh"));
    let pane = Command::new("sh")
        .arg(&script)
        .env("HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .status()
        .unwrap();
    assert!(pane.success());
    assert_eq!(
        std::fs::read_to_string(home.join("which")).unwrap().trim(),
        env!("CARGO_BIN_EXE_swarm")
    );
}
