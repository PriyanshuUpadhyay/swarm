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
    // The marker says this home is swarm's, so a test can add adapter files before any command
    // (ADR 0036).
    std::fs::write(dir.join(".swarm/swarm-home"), "swarm\n").unwrap();
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
    // The owner gave standing consent for folder trust (ADR 0043).
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
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
    // Launch starts only a runner whose CLI is on PATH (ADR 0032).
    tool(&home, "claude", "true");
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
    // The app shows the role's model before the chair writes its first log.
    assert!(
        stderr(&output).contains("model opus"),
        "{}",
        stderr(&output)
    );
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
    // Each chair gets its own session, as each app chat does.
    let chair_in = |cwd: &Path| {
        let session = swarm(&home, &fake, &["session", "new", "lane"]);
        assert!(session.status.success(), "{}", stderr(&session));
        let session = String::from_utf8(session.stdout).unwrap();
        let env = [
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", session.trim()),
            ("SWARM_AGENT_ID", "orchestrator"),
        ];
        let cwd = cwd.to_string_lossy().into_owned();
        let output = swarm(
            &home,
            &env,
            &["launch", "orchestrator", "review.deep", "--cwd", &cwd],
        );
        assert!(output.status.success(), "{}", stderr(&output));
        stderr(&output)
    };
    let git_repo = |name: &str| {
        let dir = home.join(name);
        let init = Command::new("git").arg("init").arg("-q").arg(&dir).status();
        assert!(init.unwrap().success());
        dir
    };
    let chair = git_repo("chair");
    chair_in(&chair);
    assert!(trusted(&home.join(".claude.json"), &chair));
    assert!(trusted(&profiles[0].join(".claude.json"), &chair));
    assert!(!chair.join(".herdr/workers").exists());

    // A chair in $HOME, or in a folder another account can write, gets no entry and shows
    // Claude's own trust screen.
    let shared = git_repo("shared");
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();
    for cwd in [&home, &shared] {
        let warning = chair_in(cwd);
        assert!(warning.contains("not pre-trusting for claude"), "{warning}");
        assert!(
            !trusted(&home.join(".claude.json"), cwd),
            "{}",
            cwd.display()
        );
        assert!(
            !trusted(&profiles[0].join(".claude.json"), cwd),
            "{}",
            cwd.display()
        );
    }
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
    // A text screen is read by its provider's patterns; `spawn --provider` needs an account.
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
fn the_owner_presses_only_up_and_ctrl_u_in_an_agent_pane() {
    let home = scratch("key");
    let session = answer_session(&home, "true");
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let keys = || std::fs::read_to_string(home.join("keys")).unwrap_or_default();

    let mut in_pane = app.to_vec();
    in_pane.push(("HERDR_PANE_ID", "w1:p2"));
    let output = swarm(&home, &in_pane, &["key", "seat", "Up"]);
    assert!(
        stderr(&output).contains("only the owner presses keys"),
        "{}",
        stderr(&output)
    );
    // Escape and C-c stop the agent's turn.
    for key in ["Escape", "C-c"] {
        let output = swarm(&home, &app, &["key", "seat", key]);
        assert!(
            stderr(&output).contains("only Up and C-u"),
            "{key}: {}",
            stderr(&output)
        );
    }
    assert_eq!(keys(), "");

    for key in ["Up", "C-u"] {
        let output = swarm(&home, &app, &["key", "seat", key]);
        assert!(output.status.success(), "{key}: {}", stderr(&output));
    }
    assert_eq!(keys(), "Up\nC-u\n");

    let output = swarm(&home, &app, &["key"]);
    assert!(
        stderr(&output).contains("| key <agent_id> <Up|C-u> |"),
        "{}",
        stderr(&output)
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
fn a_seat_with_no_provider_shows_the_state_that_herdr_reports() {
    let home = scratch("no-provider");
    let session = answer_session(&home, "true");
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    connection
        .execute("UPDATE agent SET provider = NULL WHERE id = 'seat'", [])
        .unwrap();
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let seat = |screen: &str| {
        std::fs::write(home.join("screen"), screen).unwrap();
        let output = swarm(&home, &chair, &["agents", "--json"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        list["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == "seat")
            .unwrap()
            .clone()
    };
    // A text screen (tmux) needs a known provider to be read, so the state stays unset.
    let text = seat(PERMISSION);
    assert_eq!(text["state"], serde_json::Value::Null);
    assert_eq!(text["prompt"], serde_json::Value::Null);
    // Herdr's status names no provider.
    let working = seat(r#"{"result":{"agent":{"agent_status":"working"}}}"#);
    assert_eq!(working["state"], "working");
    assert_eq!(working["state_source"], "screen");
    let idle = seat(r#"{"result":{"agent":{"agent_status":"idle"}}}"#);
    assert_eq!(idle["state"], "done");
    let blocked = seat(r#"{"result":{"agent":{"agent_status":"blocked"}}}"#);
    assert_eq!(blocked["state"], "waiting");
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

fn chair(session: &str) -> [(&'static str, &str); 3] {
    [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session),
        ("SWARM_AGENT_ID", "orchestrator"),
    ]
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
    // This swarm is installed next to other tools, as in ~/.cargo/bin, and the owner's PATH
    // picks another install of one of them.
    let install = home.join("install");
    std::fs::create_dir_all(&install).unwrap();
    let launcher = install.join("swarm");
    std::fs::copy(env!("CARGO_BIN_EXE_swarm"), &launcher).unwrap();
    tool(&home, "cargo", "true");
    std::fs::copy(home.join("bin/cargo"), install.join("cargo")).unwrap();
    let output = Command::new(&launcher)
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .envs(chair)
        .current_dir(&home)
        .args([
            "spawn",
            "seat",
            "coder",
            "--",
            "sh",
            "-c",
            // One name per call: dash's `command -v` prints only its first name.
            "command -v swarm > \"$HOME/which\"; command -v cargo >> \"$HOME/which\"",
        ])
        .output()
        .unwrap();
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
    let which = std::fs::read_to_string(home.join("which")).unwrap();
    let [swarm, cargo] = which.lines().collect::<Vec<_>>()[..] else {
        panic!("{which}");
    };
    assert_eq!(std::fs::canonicalize(swarm).unwrap(), launcher);
    assert_eq!(Path::new(cargo), home.join("bin/cargo"));
    // An agent in that pane launches the next one through the link, which must still lead to
    // the launching binary and not to itself.
    let again = Command::new(swarm)
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .envs(chair)
        .current_dir(&home)
        .args(["spawn", "next", "coder", "--", "true"])
        .output()
        .unwrap();
    assert!(again.status.success(), "{}", stderr(&again));
    assert_eq!(std::fs::canonicalize(swarm).unwrap(), launcher);
}

/// New Chat with no pick runs the chat profile with its effort; a one-off pick runs exactly that
/// model with the chat profile's effort for that provider (ADR 0033).
#[test]
fn a_chat_launches_from_the_chat_profile_and_a_one_off_pick_keeps_its_effort() {
    let home = scratch("chat");
    tool(&home, "claude", "true");
    tool(&home, "codex", "true");
    tool(
        &home,
        "yelo",
        r#"echo '[{"name":"personal","dir":"/p/personal","signed_in":true,"remaining":50}]'"#,
    );
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
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let script = |seat: &str| {
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/{seat}.sh"))).unwrap()
    };

    let profile = swarm(
        &home,
        &env,
        &["launch", "chat-profile", "chat", "--cwd", &cwd],
    );
    assert!(profile.status.success(), "{}", stderr(&profile));
    assert!(stderr(&profile).contains("swarm: chat: running claude/opus/high"));
    let command = script("chat-profile");
    assert!(
        command.contains("'--model' 'opus' '--effort' 'high'"),
        "{command}"
    );

    let one_off = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-one-off",
            "chat",
            "--provider",
            "codex",
            "--model",
            "gpt-6-luna",
            "--cwd",
            &cwd,
        ],
    );
    assert!(one_off.status.success(), "{}", stderr(&one_off));
    let command = script("chat-one-off");
    assert!(command.contains("'--model' 'gpt-6-luna'"), "{command}");
    assert!(
        command.contains(r#"'model_reasoning_effort="high"'"#),
        "{command}"
    );
    assert!(
        command.contains("'--sandbox' 'workspace-write'"),
        "{command}"
    );

    // The app always sends `--account auto` for the chat profile; a provider with no accounts
    // launches on its own login instead of failing.
    tool(&home, "agy", "true");
    let agy = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-agy",
            "chat",
            "--provider",
            "agy",
            "--model",
            "flash",
            "--account",
            "auto",
            "--cwd",
            &cwd,
        ],
    );
    assert!(agy.status.success(), "{}", stderr(&agy));
    assert!(script("chat-agy").contains("'agy' '--model' 'flash'"));

    // A named account that no runner's provider has skips each runner before anything runs.
    let named = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-named",
            "chat",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(!named.status.success());
    let text = stderr(&named);
    assert!(
        text.contains("1 claude/opus/high: no claude account named work"),
        "{text}"
    );
    assert!(text.contains("no runner can run"), "{text}");
    assert!(!text.contains("running"), "{text}");

    // On a Mac without yelo, `auto` falls back to the CLI's own login.
    std::fs::remove_file(home.join("bin/yelo")).unwrap();
    let no_yelo = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-no-yelo",
            "chat",
            "--account",
            "auto",
            "--cwd",
            &cwd,
        ],
    );
    assert!(no_yelo.status.success(), "{}", stderr(&no_yelo));
    assert!(stderr(&no_yelo).contains("claude uses its own login"));
    assert!(script("chat-no-yelo").contains("'--model' 'opus' '--effort' 'high'"));

    // A stuck yelo costs `auto` its 2 s read limit, not the whole launch.
    tool(&home, "yelo", "sleep 8");
    let started = std::time::Instant::now();
    let stuck = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-stuck-yelo",
            "chat",
            "--account",
            "auto",
            "--cwd",
            &cwd,
        ],
    );
    assert!(stuck.status.success(), "{}", stderr(&stuck));
    assert!(started.elapsed() < std::time::Duration::from_secs(8));
    assert!(
        stderr(&stuck).contains("yelo did not answer within 2 s; claude uses its own login"),
        "{}",
        stderr(&stuck)
    );
}

#[test]
fn all_agent_listings_match_each_sessions_adapter_and_fields() {
    let home = scratch("all-agents");
    let adapters = home.join(".swarm/adapters");
    std::fs::create_dir_all(&adapters).unwrap();
    for (name, list) in [
        ("live", "printf live-pane"),
        ("dead", "true"),
        ("failed", "exit 1"),
    ] {
        std::fs::write(adapters.join(format!("{name}.conf")), format!(
            "self = true\nspawn = true\nring = true\nlist = test -d \"$HOME/sessions/$SWARM_SESSION_ID\" && test \"$SWARM_ADAPTER\" = {name} && test \"$SWARM_AGENT_ID\" = orchestrator || exit 1; {list}\nclose = true\ncapture = true\nscreen = cat \"$HOME/screen\"\nattach = true\n"
        )).unwrap();
    }
    std::fs::write(home.join("screen"), PERMISSION).unwrap();
    let log = home.join("agent.jsonl");
    std::fs::write(&log, "").unwrap();
    let mut connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut expected = serde_json::Map::new();
    for name in ["live", "dead", "failed"] {
        let session =
            swarm::store::create_session(&connection, "lane", &home, None, Some(name)).unwrap();
        std::fs::create_dir_all(home.join("sessions").join(&session)).unwrap();
        for (id, pane) in [
            ("coder", format!("{name}-pane")),
            ("unspawned", String::new()),
        ] {
            swarm::store::add_agent(&connection, &session, id, "code").unwrap();
            if !pane.is_empty() {
                swarm::store::set_pane(&connection, &session, id, &pane).unwrap();
            }
            swarm::store::set_provider(&connection, &session, id, "claude").unwrap();
            swarm::store::set_log(&connection, &session, id, &log).unwrap();
            swarm::store::set_state(
                &connection,
                &session,
                id,
                "failed",
                "hook",
                Some("fixture failure"),
                now,
            )
            .unwrap();
        }
        let output = swarm(
            &home,
            &[
                ("SWARM_ADAPTER", name),
                ("SWARM_SESSION_ID", &session),
                ("SWARM_AGENT_ID", "orchestrator"),
            ],
            &["agents", "--json"],
        );
        assert!(output.status.success(), "{}", stderr(&output));
        let listing: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            listing["agents"][0]["alive"],
            match name {
                "live" => serde_json::json!(true),
                "dead" => serde_json::json!(false),
                _ => serde_json::Value::Null,
            }
        );
        if name == "live" {
            assert!(listing["agents"][0]["prompt"].is_object());
        }
        expected.insert(session, listing);
    }
    // Unknown adapters and old sessions without an adapter must not borrow the process adapter.
    for adapter in [None, Some(" "), Some("missing"), Some("dead")] {
        let session =
            swarm::store::create_session(&connection, "lane", &home, None, adapter).unwrap();
        swarm::store::add_agent(&connection, &session, "coder", "code").unwrap();
        if adapter == Some("dead") {
            swarm::store::archive_sessions(&mut connection, &[session]).unwrap();
        }
    }
    swarm::store::create_session(&connection, "lane", &home, None, Some("live")).unwrap();
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "missing"),
            ("SWARM_SESSION_ID", "wrong-session"),
            ("SWARM_AGENT_ID", "wrong-agent"),
        ],
        &["agents", "--json", "--all"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, serde_json::Value::Object(expected));
}

/// A sweep pass that fails after it reported a dead child still prints `dead <id>`. The report
/// forgot the child's pane, so no later pass finds it dead again.
#[test]
fn a_failed_sweep_pass_still_prints_the_dead_child_it_reported() {
    let home = scratch("sweep-dead-line");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    // The first listing shows only the chair and `second`; the next one fails.
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\n\
         list = [ -e \"$HOME/listed\" ] && exit 1; touch \"$HOME/listed\"; printf 'chair p2\\n'\n\
         close = true\ncapture = true\n",
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
    for args in [
        &["agent", "add", "orchestrator", "orchestrator"][..],
        &["agent", "add", "first", "coder"],
        &["agent", "add", "second", "coder"],
    ] {
        let output = swarm(&home, &chair(&session), args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_pane(&connection, &session, "first", "p1").unwrap();
    swarm::store::set_pane(&connection, &session, "second", "p2").unwrap();

    let sweep = swarm(&home, &chair(&session), &["sweep"]);
    assert!(!sweep.status.success());
    assert_eq!(String::from_utf8_lossy(&sweep.stdout), "dead first\n");
}

/// A home with a fake adapter, a `codex` and a `claude` on PATH, and routes `review.deep` to Codex
/// and `chair` to Claude, with one chair session. Returns the chair's env.
fn trust_session(home: &Path) -> [(&'static str, String); 3] {
    tool(home, "codex", "true");
    tool(home, "claude", "true");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"review.deep": ["codex-x"], "chair": ["claude-opus"]},
            "runners": {"codex-x": {"provider": "codex", "model": "gpt-6-luna", "effort": "high"},
                        "claude-opus": {"provider": "claude", "model": "opus", "effort": "high"}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
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
    [
        ("SWARM_ADAPTER", "fake".to_string()),
        ("SWARM_SESSION_ID", session),
        ("SWARM_AGENT_ID", "orchestrator".to_string()),
    ]
}

fn git_repo(home: &Path, name: &str) -> PathBuf {
    let dir = home.join(name);
    assert!(
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    dir
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

fn recorded_trust(home: &Path) -> Vec<serde_json::Value> {
    let output = swarm(home, &[], &["managed", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["writer"] == "launch.trust" && entry["recorded"] == true)
        .cloned()
        .collect()
}

/// With no consent (ADR 0043), a seat's launch writes no trust entry. It prints the bare
/// `trust-pending` line the app reads, the diff it would write, and the command that approves it,
/// and the seat still starts, at its CLI's own trust prompt.
#[test]
fn a_seat_launch_with_no_consent_writes_no_trust_and_prints_the_plan() {
    let home = scratch("ask");
    let env = trust_session(&home);
    let repo = git_repo(&home, "app");
    let config = home.join(".codex/config.toml");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(&config, "model = \"gpt\"\n").unwrap();

    let output = launch_in(&home, &env, "seat", "review.deep", &repo);
    let err = stderr(&output);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "model = \"gpt\"\n"
    );
    assert!(
        err.lines()
            .any(|line| line == format!("trust-pending codex {}", repo.display())),
        "{err}"
    );
    assert!(
        err.contains(&format!("+[projects.\"{}\"]", repo.display())),
        "{err}"
    );
    assert!(err.contains("+trust_level = \"trusted\""), "{err}");
    assert!(
        err.contains(&format!("swarm setup --plan --cwd {}", repo.display())),
        "{err}"
    );
    assert!(recorded_trust(&home).is_empty());

    // An unreadable consent file is no consent (R4), and swarm says so.
    std::fs::write(home.join(".swarm/consent.json"), "{not json").unwrap();
    let err = stderr(&launch_in(&home, &env, "seat-2", "review.deep", &repo));
    assert!(err.contains("consent"), "{err}");
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "model = \"gpt\"\n"
    );

    // Standing consent: the entry is written, recorded, and named.
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
    let err = stderr(&launch_in(&home, &env, "seat-3", "review.deep", &repo));
    assert!(
        err.lines()
            .any(|line| line == format!("trusted codex {}", repo.display())),
        "{err}"
    );
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("trust_level = \"trusted\"")
    );
    let recorded = recorded_trust(&home);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0]["state"], "present");
}

/// The app makes a chair launch in the folder the owner picked, and hides its pane, so with no
/// consent that pick is consent for that one folder (owner answer I1). The write is recorded and
/// shown, with its diff, never made silently.
#[test]
fn a_chair_launch_trusts_its_picked_folder_and_shows_the_write() {
    let home = scratch("chair-pick");
    let env = trust_session(&home);
    let repo = git_repo(&home, "picked");

    let err = stderr(&launch_in(&home, &env, "orchestrator", "chair", &repo));
    assert!(trusted(&home.join(".claude.json"), &repo));
    assert!(
        err.lines()
            .any(|line| line == format!("trusted claude {}", repo.display())),
        "{err}"
    );
    assert!(
        err.contains("+      \"hasTrustDialogAccepted\": true"),
        "{err}"
    );
    let recorded = recorded_trust(&home);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0]["path"][1], repo.to_string_lossy().as_ref());

    // A Claude `false` is replaced, and the record keeps it for revert (Q4).
    let other = git_repo(&home, "seen");
    let claude = home.join(".claude.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude).unwrap()).unwrap();
    value["projects"][other.to_string_lossy().as_ref()] =
        serde_json::json!({"hasTrustDialogAccepted": false});
    std::fs::write(&claude, value.to_string()).unwrap();
    // Each chair gets its own session, as each app chat does.
    let env = trust_session(&home);
    launch_in(&home, &env, "orchestrator", "chair", &other);
    assert!(trusted(&claude, &other));
    let before: Vec<_> = recorded_trust(&home)
        .into_iter()
        .filter(|entry| entry["path"][1] == other.to_string_lossy().as_ref())
        .map(|entry| entry["before"].clone())
        .collect();
    assert_eq!(before, [serde_json::json!(false)]);
}
