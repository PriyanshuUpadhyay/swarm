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
    let profiles = ["work", "personal"].map(|name| home.join(".claude/.profiles").join(name));
    for dir in &profiles {
        std::fs::create_dir_all(dir).unwrap();
    }
    let hidden = home.join(".claude/.profiles/.session-map");
    std::fs::create_dir_all(&hidden).unwrap();
    tool(
        &home,
        "claude",
        r#"if [ "$*" = 'auth status' ]; then /bin/echo "$CLAUDE_CONFIG_DIR" >> "$HOME/native-reads"; echo '{"loggedIn":true}'; fi"#,
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let accounts: Vec<_> = profiles.iter().zip([30,75]).map(|(path,used)| serde_json::json!({"home":path,"meters":[{"provider":"claude","account":path.file_name().unwrap().to_str().unwrap(),"label":"account","window":"5h","window_minutes":300,"used_pct":used,"reset_time_seconds":null,"state":"fresh","source":"claude_api","reason":null,"as_of_seconds":now}]})).collect();
    std::fs::write(
        home.join(".swarm/claude-usage.json"),
        serde_json::json!({"accounts":accounts}).to_string(),
    )
    .unwrap();
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
    // Consent covers only a folder that passes the trust check, such as a git repo.
    let launch = |seat: &str, repo: &str, account: Option<&str>| {
        let cwd = git_repo(&home, repo).to_string_lossy().into_owned();
        let mut args = vec!["launch", seat, "review.deep", "--cwd", &cwd];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        let output = swarm(&home, &env, &args);
        assert!(output.status.success(), "{seat}: {}", stderr(&output));
        if account.is_none() {
            assert!(
                stderr(&output).contains("account work"),
                "{}",
                stderr(&output)
            );
        }
        home.join(repo).join(".herdr/workers")
    };

    let any = launch("seat-any", "any", None);
    assert!(trusted(&home.join(".claude.json"), &any));
    assert!(!hidden.join(".claude.json").exists());
    let only_b = launch("seat-personal", "only-personal", Some("personal"));
    assert!(trusted(&home.join(".claude.json"), &only_b));
    for dir in &profiles {
        assert!(!dir.join(".claude.json").exists());
    }

    let native_default = launch("seat-default", "only-default", Some("default"));
    assert!(trusted(&home.join(".claude.json"), &native_default));
    assert!(!trusted(
        &home.join(".claude/.claude.json"),
        &native_default
    ));
    let default_script =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/seat-default.sh")))
            .unwrap();
    assert!(
        default_script.contains("'env' '-u' 'CLAUDE_CONFIG_DIR' "),
        "{default_script}"
    );
    assert!(
        !default_script.contains("CLAUDE_CONFIG_DIR="),
        "{default_script}"
    );

    // Auto and the pane use the same native account home.
    std::fs::write(home.join("native-reads"), "").unwrap();
    let cwd = git_repo(&home, "auto");
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
    let native_reads = std::fs::read_to_string(home.join("native-reads")).unwrap();
    assert_eq!(
        native_reads.lines().count(),
        3,
        "the role probe, Auto and pane must share one native account read"
    );
    assert!(
        stderr(&output).contains("account work"),
        "{}",
        stderr(&output)
    );
    // The app shows the role's model before the chair writes its first log.
    assert!(
        stderr(&output).contains("model opus"),
        "{}",
        stderr(&output)
    );
    assert!(trusted(
        &home.join(".claude.json"),
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
        let cwd = git_repo(&home, repo);
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
    let chair = git_repo(&home, "chair");
    chair_in(&chair);
    assert!(trusted(&home.join(".claude.json"), &chair));
    assert!(!profiles[0].join(".claude.json").exists());
    assert!(!chair.join(".herdr/workers").exists());

    // A chair in $HOME, or in a folder another account can write, gets no entry and shows
    // Claude's own trust screen.
    let shared = git_repo(&home, "shared");
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
        command.contains(r#"'default_permissions="swarm-code"'"#),
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

    // With unavailable native auth, `auto` falls back to the CLI's own login.
    let no_usage = swarm(
        &home,
        &env,
        &["launch", "chat-no-usage", "chat", "--cwd", &cwd],
    );
    assert!(no_usage.status.success(), "{}", stderr(&no_usage));
    assert!(stderr(&no_usage).contains("claude uses its own login"));
    assert!(
        script("chat-no-usage").contains(
            "'env' '-u' 'CLAUDE_CONFIG_DIR' '-u' 'CLAUDE_SECURESTORAGE_CONFIG_DIR' '-u' 'AGENT_PROFILE_LABEL' '--' 'claude'"
        )
    );
    assert!(script("chat-no-usage").contains("'--model' 'opus' '--effort' 'high'"));

    // An unavailable native read keeps the scheduler fail-open and Auto on the CLI login.
    tool(
        &home,
        "claude",
        "if [ \"$*\" = 'auth status' ]; then exec /bin/sleep 8; fi",
    );
    let started = std::time::Instant::now();
    let stuck = swarm(
        &home,
        &env,
        &[
            "launch",
            "chat-stuck-auth",
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
        stderr(&stuck).contains("no automatic account for claude; claude uses its own login"),
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
                None,
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
/// `trust-pending` line the chair reads (the app reads only `trusted` lines), the diff it would
/// write, and the command that approves it, and the seat still starts, at its CLI's own trust
/// prompt.
#[test]
fn a_seat_launch_with_no_consent_writes_no_trust_and_prints_the_plan() {
    let home = scratch("ask");
    let env = trust_session(&home);
    let repo = git_repo(&home, "my app");
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
        err.contains(&format!(
            "swarm setup trust --plan --cwd '{}'",
            repo.display()
        )),
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

/// AGY has one settings file, and no account to fall back on, so a standing-consent launch that
/// cannot write its trust entry stops, as before consent (03-contracts section 7).
#[test]
fn an_agy_trust_write_that_fails_stops_the_launch() {
    let home = scratch("agy-fail");
    let env = trust_session(&home);
    tool(&home, "agy", "true");
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"agy.seat": ["agy-flash"]},
            "runners": {"agy-flash": {"provider": "agy", "model": "flash"}}}"#,
    )
    .unwrap();
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
    let settings = home.join(".gemini/antigravity-cli/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "[]\n").unwrap();
    let repo = git_repo(&home, "app");
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();

    let cwd = repo.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &["launch", "agy-seat", "agy.seat", "--cwd", &cwd],
    );
    assert!(!output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("not a JSON object"),
        "{}",
        stderr(&output)
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), "[]\n");
}

/// Standing consent covers only a folder that passes the trust check (ADR 0043), so a Claude
/// seat in a plain folder gets no entry and asks in its pane.
#[test]
fn a_claude_seat_in_a_folder_that_fails_the_trust_check_gets_no_entry() {
    let home = scratch("seat-loose");
    let env = trust_session(&home);
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
    let loose = home.join("loose");
    std::fs::create_dir_all(&loose).unwrap();

    let err = stderr(&launch_in(&home, &env, "seat", "chair", &loose));
    assert!(
        !trusted(&home.join(".claude.json"), &loose.join(".herdr/workers")),
        "{err}"
    );
    assert!(recorded_trust(&home).is_empty(), "{err}");
    assert!(err.contains("not pre-trusting for claude"), "{err}");
}

/// The app reads a launch's stderr for bare `model`, `account`, and `trusted` lines. A chair's
/// trust diff prints next to them, and each of its lines keeps its diff mark, so a context line
/// such as ` model = "gpt-5.5"` is never a key line (L-12).
#[test]
fn a_chair_trust_diff_never_prints_a_bare_key_line() {
    let home = scratch("chair-diff");
    let env = trust_session(&home);
    let repo = git_repo(&home, "picked");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"gpt-5.5\"\n").unwrap();

    let err = stderr(&launch_in(
        &home,
        &env,
        "orchestrator",
        "review.deep",
        &repo,
    ));
    assert!(err.contains("\n model = \"gpt-5.5\"\n"), "{err}");
    let keys: Vec<&str> = err
        .lines()
        .filter(|line| line.starts_with("model ") || line.starts_with("trusted "))
        .collect();
    assert_eq!(
        keys,
        [
            format!("trusted codex {}", repo.display()).as_str(),
            "model gpt-6-luna"
        ],
        "{err}"
    );
}

/// A resuming Claude seat runs in cwd itself (`claude_child`), so with no consent its launch
/// reports cwd as pending, and the approve command it prints plans the Claude entry for that same
/// folder; once applied, the next resumed launch has nothing pending.
#[test]
fn a_resumed_claude_seats_approve_command_plans_the_folder_it_reports() {
    let home = scratch("resume-ask");
    let env = trust_session(&home);
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let launch = |seat: &str| {
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let args = [
            "launch", seat, "chair", "--cwd", &cwd, "--", "--resume", "abc",
        ];
        let output = swarm(&home, &env, &args);
        assert!(output.status.success(), "{}", stderr(&output));
        stderr(&output)
    };

    let err = launch("seat");
    assert!(
        err.lines()
            .any(|line| line == format!("trust-pending claude {cwd}")),
        "{err}"
    );
    let approve = format!("swarm setup trust --plan --cwd '{cwd}' --resume");
    assert!(err.contains(&approve), "{err}");

    let plan = swarm(
        &home,
        &[],
        &[
            "setup", "trust", "--plan", "--json", "--cwd", &cwd, "--resume",
        ],
    );
    assert!(plan.status.success(), "{}", stderr(&plan));
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    let applied = swarm(
        &home,
        &[],
        &[
            "setup", "trust", "--digest", digest, "--cwd", &cwd, "--resume",
        ],
    );
    assert!(applied.status.success(), "{}", stderr(&applied));
    assert!(trusted(&home.join(".claude.json"), &repo));
    assert!(!trusted(
        &home.join(".claude.json"),
        &repo.join(".herdr/workers")
    ));

    let err = launch("seat-2");
    assert!(!err.contains("trust-pending claude"), "{err}");
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
fn invalid_codex_home_keeps_real_auth_read_failure_fail_open() {
    let home = scratch("work-invalid-home-read-failure");
    trust_session(&home);
    std::fs::create_dir_all(home.join(".codex-Work")).unwrap();
    tool(&home, "codex", "exit 1");
    let output = swarm(&home, &[], &["roles", "get", "review.deep"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let resolved: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(resolved["runnerId"], "review.deep#1");
    assert_eq!(resolved["skipped"], serde_json::json!([]));
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

#[test]
fn named_launch_reuses_the_role_probe_deadline() {
    let home = scratch("work-account-deadline");
    std::fs::create_dir_all(home.join(".claude/.profiles/work")).unwrap();
    tool(
        &home,
        "claude",
        "echo probe >> \"$HOME/native-reads\"; exec /bin/sleep 25",
    );
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    ).unwrap();
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
    let start = std::time::Instant::now();
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "review.deep",
            "--provider",
            "claude",
            "--account",
            "work",
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("authentication is unavailable"),
        "{}",
        stderr(&output)
    );
    assert!(
        start.elapsed() < std::time::Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
    // One read per home (default and work), all started at once under the one deadline.
    assert_eq!(
        std::fs::read_to_string(home.join("native-reads"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

fn account_launch_fixture(role: &str) -> (PathBuf, String, String) {
    let home = scratch(role);
    std::fs::create_dir_all(home.join(".claude/.profiles/work")).unwrap();
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust":"standing"}"#).unwrap();
    tool(
        &home,
        "claude",
        r#"echo "$CLAUDE_CONFIG_DIR" >> "$HOME/native-reads"; /bin/sleep 0.05; echo '{"loggedIn":true}'"#,
    );
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(home.join(".swarm/adapters/fake.conf"), "self = printf chair\nspawn = printf pane-work\nring = true\nlist = true\nclose = true\ncapture = true\n").unwrap();
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
    let cwd = git_repo(&home, "work-project")
        .to_string_lossy()
        .into_owned();
    (home, session, cwd)
}

fn fable_launch_cache(home: &Path, work_fb: i64, default_fb: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let accounts: Vec<_> = [
        ("work", ".claude/.profiles/work", 10, work_fb),
        ("default", ".claude", 50, default_fb),
    ]
    .into_iter()
    .map(|(name, dir, general, fb)| {
        let meters: Vec<_> = [("5h", general), ("fb", fb)]
            .into_iter()
            .map(|(window, used)| {
                serde_json::json!({
                    "provider":"claude", "account":name, "label":name, "window":window,
                    "used_pct":used, "state":"fresh", "as_of_seconds":now,
                    "reset_time_seconds":now+3600
                })
            })
            .collect();
        serde_json::json!({"home":home.join(dir), "meters":meters})
    })
    .collect();
    std::fs::write(
        home.join(".swarm/claude-usage.json"),
        serde_json::json!({"accounts":accounts}).to_string(),
    )
    .unwrap();
}

#[test]
fn launch_uses_model_usage_and_refuses_spent_fable_auto_before_starting_a_pane() {
    let (home, session, cwd) = account_launch_fixture("fable-model-launch");
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let launch = |seat: &str, model: &str, account: Option<&str>| {
        let mut args = vec![
            "launch",
            seat,
            "chat",
            "--provider",
            "claude",
            "--model",
            model,
            "--cwd",
            &cwd,
        ];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        swarm(&home, &env, &args)
    };
    fable_launch_cache(&home, 100, 100);
    let refused = launch("spent", "best", None);
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("every signed-in Claude account has used its Fable limit"));
    assert!(
        !home
            .join(format!(".swarm/runs/{session}/spent.sh"))
            .exists()
    );
    let connection = rusqlite::Connection::open(home.join(".swarm/swarm.db")).unwrap();
    let started: i64 = connection
        .query_row("SELECT count(*) FROM agent WHERE id = 'spent'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(started, 0);
    let named = launch("named", "best", Some("work"));
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(stderr(&named).contains("account work"));
    let sonnet = launch("general", "sonnet", None);
    assert!(sonnet.status.success(), "{}", stderr(&sonnet));
    assert!(stderr(&sonnet).contains("account work"));
    fable_launch_cache(&home, 95, 10);
    let fable = launch("unspent", "best", None);
    assert!(fable.status.success(), "{}", stderr(&fable));
    assert!(stderr(&fable).contains("account default"));
}

fn renewable_launch_cache(home: &Path, work_used: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let work = serde_json::json!({
        "provider":"claude", "account":"work", "label":"work", "window":"5h",
        "used_pct":work_used, "state":"fresh", "as_of_seconds":now,
        "reset_time_seconds":now+3600
    });
    let default = serde_json::json!({
        "provider":"claude", "account":"default", "label":"default",
        "state":"failed", "reason":"Token expired", "reason_code":"token_expired"
    });
    std::fs::write(
        home.join(".swarm/claude-usage.json"),
        serde_json::json!({"accounts":[
            {"home":home.join(".claude/.profiles/work"), "meters":[work]},
            {"home":home.join(".claude"), "meters":[default]},
        ]})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn auto_takes_a_renewable_account_only_when_no_fresh_one_has_enough_left() {
    let (home, session, cwd) = account_launch_fixture("renewable-pick");
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let launch = |seat: &str| {
        swarm(
            &home,
            &env,
            &[
                "launch",
                seat,
                "chat",
                "--provider",
                "claude",
                "--model",
                "sonnet",
                "--cwd",
                &cwd,
            ],
        )
    };
    renewable_launch_cache(&home, 60);
    let fresh = launch("fresh");
    assert!(fresh.status.success(), "{}", stderr(&fresh));
    assert!(
        stderr(&fresh).contains("account work"),
        "{}",
        stderr(&fresh)
    );
    renewable_launch_cache(&home, 97);
    let renewable = launch("renewable");
    assert!(renewable.status.success(), "{}", stderr(&renewable));
    assert!(
        stderr(&renewable).contains("account default"),
        "{}",
        stderr(&renewable)
    );
    let role = swarm(&home, &[], &["roles", "get", "chat"]);
    assert!(role.status.success(), "{}", stderr(&role));
    let role: serde_json::Value = serde_json::from_slice(&role.stdout).unwrap();
    assert_eq!(role["provider"], "claude");
    assert_eq!(role["skipped"], serde_json::json!([]));
}

#[test]
fn an_auto_launch_warns_when_a_trust_file_cannot_be_edited_and_a_named_one_stops() {
    let (home, session, cwd) = account_launch_fixture("trust-file-auto");
    std::fs::write(home.join(".claude.json"), "[]\n").unwrap();
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let launch = |seat: &str, account: Option<&str>| {
        let mut args = vec![
            "launch",
            seat,
            "chat",
            "--provider",
            "claude",
            "--model",
            "sonnet",
            "--cwd",
            &cwd,
        ];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        swarm(&home, &env, &args)
    };
    for (seat, account) in [("plain", None), ("auto", Some("auto"))] {
        let output = launch(seat, account);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(
            stderr(&output).contains("swarm: skipped"),
            "{}",
            stderr(&output)
        );
    }
    let named = launch("named", Some("work"));
    assert!(!named.status.success());
    assert!(
        stderr(&named).contains("not a JSON object"),
        "{}",
        stderr(&named)
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".claude.json")).unwrap(),
        "[]\n"
    );
}

#[test]
fn roles_check_uses_fable_models_and_reads_native_accounts_once_per_provider() {
    let (home, session, cwd) = account_launch_fixture("fable-role-fallback");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes":{"review.scope":["claude-fable","claude-sonnet"]},"runners":{
            "claude-fable":{"provider":"claude","model":"fable","effort":"high"},
            "claude-sonnet":{"provider":"claude","model":"sonnet","effort":"high"}}}"#,
    )
    .unwrap();
    fable_launch_cache(&home, 100, 100);
    let output = swarm(&home, &[], &["roles", "check", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let check: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let scope = check["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["name"] == "review.scope")
        .unwrap();
    assert_eq!(scope["pick"], 1);
    assert_eq!(scope["skipped"][0]["code"], "fable_spent");
    assert_eq!(
        scope["skipped"][0]["text"],
        "every signed-in Claude account has used its Fable limit"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("native-reads"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(
        &home,
        &env,
        &["launch", "fallback", "review.scope", "--cwd", &cwd],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("running claude/sonnet/high"));
    assert!(stderr(&output).contains("account work"));
    let named = swarm(
        &home,
        &env,
        &[
            "launch",
            "named",
            "review.scope",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(stderr(&named).contains("running claude/fable/high"));
    fable_launch_cache(&home, 99, 99);
    let output = swarm(&home, &[], &["roles", "get", "review.scope"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let role: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(role["model"], "sonnet");
    assert!(
        role["skipped"][0]["text"]
            .as_str()
            .unwrap()
            .contains("usage 1% left")
    );
}

#[test]
fn slow_working_auth_is_read_once_by_role_and_named_launch() {
    let (home, session, cwd) = account_launch_fixture("work-slow-probe");
    std::fs::create_dir_all(home.join(".claude/.profiles/personal")).unwrap();
    tool(
        &home,
        "claude",
        r#"echo "$CLAUDE_CONFIG_DIR" >> "$HOME/native-reads"; /bin/sleep 0.1; echo '{"loggedIn":true}'"#,
    );
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "review.deep",
            "--provider",
            "claude",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("account work"));
    assert_eq!(
        std::fs::read_to_string(home.join("native-reads"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn account_budget_starts_after_the_herdr_check() {
    let (home, session, cwd) = account_launch_fixture("personal-herdr-budget");
    let adapter = std::fs::read(home.join(".swarm/adapters/fake.conf")).unwrap();
    std::fs::write(home.join(".swarm/adapters/herdr.conf"), adapter).unwrap();
    tool(
        &home,
        "herdr",
        "test \"$*\" = status || exit 8; /bin/sleep 2.2",
    );
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "herdr"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "chat",
            "--model",
            "opus",
            "--provider",
            "claude",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("account work"));
}

#[test]
fn account_budget_starts_after_a_waiting_store_open() {
    let (home, session, cwd) = account_launch_fixture("spare-store-budget");
    let connection = rusqlite::Connection::open(home.join(".swarm/swarm.db")).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE;")
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2200));
        connection.execute_batch("COMMIT;").unwrap();
    });
    let started = std::time::Instant::now();
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "chat",
            "--model",
            "opus",
            "--provider",
            "claude",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    release.join().unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_secs(2));
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("account work"));
}

#[test]
fn a_superset_chair_registers_as_orchestrator_on_session_new() {
    let home = scratch("superset-chair");
    let superset = [("SUPERSET_TERMINAL_ID", "t1")];
    let session = swarm(&home, &superset, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let inbox = swarm(&home, &superset, &["inbox"]);
    assert!(inbox.status.success(), "{}", stderr(&inbox));
}

#[test]
fn a_superset_terminal_counts_as_an_agent_pane_for_the_owner_only_guard() {
    let home = scratch("superset-guard");
    let output = swarm(
        &home,
        &[("SUPERSET_TERMINAL_ID", "t1")],
        &["answer", "w1", "p1", "1"],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("only the owner answers"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_rex_chair_registers_as_orchestrator_on_session_new() {
    let home = scratch("rex-chair");
    let rex = [("REX_BLOCK", "block:1"), ("TERM_PROGRAM", "rex")];
    let session = swarm(&home, &rex, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let inbox = swarm(&home, &rex, &["inbox"]);
    assert!(inbox.status.success(), "{}", stderr(&inbox));
}

#[test]
fn a_rex_block_counts_as_an_agent_pane_for_the_owner_only_guard() {
    let home = scratch("rex-guard");
    let output = swarm(
        &home,
        &[("REX_BLOCK", "block:1")],
        &["answer", "w1", "p1", "1"],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("only the owner answers"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn claude_launch_uses_shared_chats_and_bypasses_shell_functions() {
    let (home, session, cwd) = account_launch_fixture("launch-env");
    tool(
        &home,
        "claude",
        r#"if [ "$*" = 'auth status' ]; then echo '{"loggedIn":true}'; else /usr/bin/env > "$HOME/launch-env"; fi"#,
    );
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &[
            "launch",
            "seat",
            "review.deep",
            "--provider",
            "claude",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let script = home.join(format!(".swarm/runs/{session}/seat.sh"));
    let output = Command::new("/bin/sh")
        .env_clear()
        .env("HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .env("CLAUDE_CONFIG_DIR", "/wrong")
        .args([
            "-c",
            "claude() { echo wrong > \"$HOME/function-ran\"; }; . \"$1\"",
            "test",
            script.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let environment = std::fs::read_to_string(home.join("launch-env")).unwrap();
    assert!(
        !environment
            .lines()
            .any(|line| line.starts_with("CLAUDE_CONFIG_DIR=")),
        "{environment}"
    );
    assert!(
        environment.lines().any(|line| line
            == format!(
                "CLAUDE_SECURESTORAGE_CONFIG_DIR={}/.claude-work",
                home.display()
            )),
        "{environment}"
    );
    assert!(
        environment
            .lines()
            .any(|line| line == "AGENT_PROFILE_LABEL=work")
    );
    assert!(!home.join("function-ran").exists());
    // One `env` command and no subshell, `unset` or `export`, so a fish pane can run the line too.
    let script = std::fs::read_to_string(script).unwrap();
    let cli = script.split_once("; ").unwrap().1;
    assert!(
        cli.starts_with("'env' '-u' 'CLAUDE_CONFIG_DIR' "),
        "{script}"
    );
    assert!(
        !cli.contains("unset ") && !cli.contains("export "),
        "{script}"
    );
}

/// `swarm spawn -- <cmd>` runs the owner's own command line, so a shell function of the pane
/// still runs.
#[test]
fn a_spawned_owner_command_can_be_a_shell_function_of_the_pane() {
    let (home, session, _cwd) = account_launch_fixture("spawn-function");
    let output = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", &session),
            ("SWARM_AGENT_ID", "orchestrator"),
        ],
        &["spawn", "seat", "coder", "--", "wrapper", "one"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let script = home.join(format!(".swarm/runs/{session}/seat.sh"));
    let output = Command::new("/bin/sh")
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .args([
            "-c",
            "wrapper() { echo \"$1\" > \"$HOME/function-ran\"; }; . \"$1\"",
            "test",
            script.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(home.join("function-ran")).unwrap(),
        "one\n"
    );
}

/// pi has no hooks: its chair is known at launch from the session id swarm gives it, it reads the
/// host contract from its system prompt, and it reports its state by OSC 7501 (ADR 0086).
#[test]
fn a_pi_chair_is_named_at_launch_and_gets_the_host_contract() {
    let home = scratch("pi-chair");
    tool(&home, "pi", "true");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"pi.chair": ["pi-haiku"]},
            "runners": {"pi-haiku": {"provider": "pi", "model": "anthropic/claude-haiku-5-5", "effort": "low"}}}"#,
    )
    .unwrap();
    // The app's adapter, with verbs that touch no tmux server.
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &["launch", "orchestrator", "pi.chair", "--cwd", &cwd],
    );
    assert!(output.status.success(), "{}", stderr(&output));

    let script =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
            .unwrap();
    let id = format!("{session}-orchestrator");
    assert!(
        script.contains(&format!(
            "'env' 'PI_PROGRAM_STATUS=1' 'pi' '--model' 'anthropic/claude-haiku-5-5' '--thinking' 'low' '--session-id' '{id}' '--append-system-prompt' '[agent-host: swarm-app]"
        )),
        "{script}"
    );
    assert!(!script.contains("[agent-runtime"), "{script}");

    let sessions = swarm(&home, &[], &["sessions", "--json"]);
    let sessions: serde_json::Value = serde_json::from_slice(&sessions.stdout).unwrap();
    let row = &sessions["sessions"][0];
    assert_eq!(row["chair_provider"], "pi");
    assert_eq!(row["chair_id"], id.as_str());
}

/// Codex has no per-launch chair hook, so a Codex chair is named at launch, and its first log
/// gives its thread id. A chat with no chair provider gets no sidebar row and no transcript.
#[test]
fn a_codex_chair_is_named_at_launch_and_its_log_gives_the_thread_id() {
    let home = scratch("codex-chair");
    tool(&home, "codex", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &[
            "launch",
            "orchestrator",
            "chat",
            "--provider",
            "codex",
            "--model",
            "gpt-6-luna",
            "--cwd",
            &cwd,
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let chair = || {
        let sessions = swarm(&home, &[], &["sessions", "--json"]);
        let sessions: serde_json::Value = serde_json::from_slice(&sessions.stdout).unwrap();
        sessions["sessions"][0].clone()
    };
    assert_eq!(chair()["chair_provider"], "codex");
    assert!(chair()["chair_id"].is_null());

    let thread = "01a14385-ff51-7000-8000-000000000001";
    let log = home.join(format!("rollout-2026-10-16T12-33-37-{thread}.jsonl"));
    std::fs::write(&log, "").unwrap();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_chair_log(&connection, &session, &log).unwrap();
    assert_eq!(chair()["chair_provider"], "codex");
    assert_eq!(chair()["chair_id"], thread);
}

/// The owner's send starts a turn, so the agent lists as working at once, not when its own first
/// report comes a second or two later.
#[test]
fn a_typed_prompt_marks_the_agent_working_at_once() {
    let home = scratch("type-working");
    tool(&home, "codex", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/tmux-swarm.conf"),
        // An idle Codex input box, so the send does not wait for one to draw.
        "self = printf chair\nspawn = printf pane\nring = true\nlist = printf pane\nclose = true\ncapture = printf '› Ask'\n",
    )
    .unwrap();
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let launched = swarm(
        &home,
        &env,
        &[
            "launch",
            "orchestrator",
            "chat",
            "--provider",
            "codex",
            "--model",
            "m",
            "--cwd",
            &cwd,
        ],
    );
    assert!(launched.status.success(), "{}", stderr(&launched));
    let mut typed = Command::new(env!("CARGO_BIN_EXE_swarm"));
    typed
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .envs(app)
        .env("SWARM_SESSION_ID", &session)
        .current_dir(&home)
        .args(["type", "orchestrator"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = typed.spawn().unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"hello").unwrap();
    let typed = child.wait_with_output().unwrap();
    assert!(typed.status.success(), "{}", stderr(&typed));
    let listed = swarm(&home, &env, &["agents", "--json"]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let chair = listed["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "orchestrator")
        .unwrap();
    assert_eq!(chair["state"], "working", "{listed}");
    // Nothing is written, so the agent's own report decides the state.
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let state: Option<String> = connection
        .query_row(
            "SELECT state FROM agent WHERE session_id = ?1 AND id = 'orchestrator'",
            [&session],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, None);
}

/// `spawn` gets this session's own seat panes as `SWARM_PANES`, so a host that stacks seats
/// anchors only on its own session's panes, never on another chair's seats.
#[test]
fn spawn_names_only_this_sessions_seat_panes() {
    let home = scratch("spawn-panes");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\n\
         spawn = printf '%s:[%s]\\n' \"$SWARM_AGENT_ID\" \"$SWARM_PANES\" >> \"$HOME/spawns\"; printf 'pane-%s' \"$SWARM_AGENT_ID\"\n\
         ring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let mut sessions = Vec::new();
    for _ in 0..2 {
        let session = swarm(
            &home,
            &[("SWARM_ADAPTER", "fake")],
            &["session", "new", "lane"],
        );
        assert!(session.status.success(), "{}", stderr(&session));
        sessions.push(
            String::from_utf8(session.stdout)
                .unwrap()
                .trim()
                .to_string(),
        );
    }
    for (session, seat) in [(0, "a1"), (0, "a2"), (1, "b1"), (0, "a3")] {
        let chair = [
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", sessions[session].as_str()),
            ("SWARM_AGENT_ID", "orchestrator"),
        ];
        let output = swarm(&home, &chair, &["spawn", seat, "coder"]);
        assert!(output.status.success(), "{seat}: {}", stderr(&output));
    }
    assert_eq!(
        std::fs::read_to_string(home.join("spawns")).unwrap(),
        "a1:[]\na2:[pane-a1]\nb1:[]\na3:[pane-a1 pane-a2]\n"
    );
}
