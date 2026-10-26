// Pane spawning, command execution, and exit tests.

use super::*;

const DENIED: &str =
    r#"Error: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }"#;

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
    // A hard link opens no file for write. A copy does, and on Linux another test's fork can hold
    // that write handle while this test runs the file, so exec fails with ETXTBSY.
    // A leftover link from an earlier run is removed first, so the copy below never writes
    // through a link into the cargo binary itself.
    let _ = std::fs::remove_file(&launcher);
    std::fs::hard_link(env!("CARGO_BIN_EXE_swarm"), &launcher)
        .or_else(|_| std::fs::copy(env!("CARGO_BIN_EXE_swarm"), &launcher).map(drop))
        .unwrap();
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

/// An adapter with `run` opens the pane with the launch line as its program, so swarm does not
/// ring the line into a shell. One without `run` still rings it.
#[test]
fn a_run_verb_gets_the_launch_line_and_nothing_is_rung() {
    let home = scratch("run-verb");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    let verbs = "self = printf chair\n\
         spawn = printf pane-spawn\n\
         ring = printf '%s\\n' \"$SWARM_TEXT\" >> \"$HOME/rings\"\n\
         list = true\nclose = true\ncapture = true\n";
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        format!(
            "{verbs}run = printf '%s|%s' \"$SWARM_AGENT_ID\" \"$SWARM_COMMAND\" > \"$HOME/program\"; printf pane-run\n"
        ),
    )
    .unwrap();
    std::fs::write(home.join(".swarm/adapters/plain.conf"), verbs).unwrap();
    let mut seen = Vec::new();
    for adapter in ["fake", "plain"] {
        let session = swarm(
            &home,
            &[("SWARM_ADAPTER", adapter)],
            &["session", "new", "lane"],
        );
        assert!(session.status.success(), "{}", stderr(&session));
        let session = String::from_utf8(session.stdout)
            .unwrap()
            .trim()
            .to_string();
        let chair = [
            ("SWARM_ADAPTER", adapter),
            ("SWARM_SESSION_ID", session.as_str()),
            ("SWARM_AGENT_ID", "orchestrator"),
        ];
        let output = swarm(&home, &chair, &["spawn", "seat", "coder", "--", "true"]);
        assert!(output.status.success(), "{adapter}: {}", stderr(&output));
        seen.push(String::from_utf8(output.stdout).unwrap().trim().to_string());
    }
    assert_eq!(seen, ["pane-run", "pane-spawn"]);
    let program = std::fs::read_to_string(home.join("program")).unwrap();
    assert!(program.starts_with("seat|"), "{program}");
    assert!(
        program.contains("seat.sh'; '") && program.ends_with("'exited'"),
        "{program}"
    );
    let rings = std::fs::read_to_string(home.join("rings")).unwrap();
    assert_eq!(rings.lines().count(), 1, "{rings}");
    assert!(rings.ends_with("'exited'\n"), "{rings}");
}

/// With a `run` verb the agent can end before swarm stores its pane. `exited` runs inside that
/// pane, so it asks the adapter's `self` verb and still saves the log and reports the end.
#[test]
fn exited_finds_its_own_pane_before_the_pane_is_stored() {
    let home = scratch("exited-own-pane");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf pane-own\nspawn = printf pane-spawn\nring = true\nlist = true\nclose = true\ncapture = printf 'last words'\n",
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
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::add_agent(&connection, &session, "orchestrator", "orchestrator").unwrap();
    swarm::store::set_pane(&connection, &session, "orchestrator", "pane-chair").unwrap();
    swarm::store::add_agent(&connection, &session, "seat", "coder").unwrap();
    let seat = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "seat"),
    ];
    let exited = swarm(&home, &seat, &["exited"]);
    assert!(exited.status.success(), "{}", stderr(&exited));
    assert_eq!(
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/seat.log"))).unwrap(),
        "last words"
    );
    let unknown = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "ghost"),
    ];
    let refused = swarm(&home, &unknown, &["exited"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("no agent ghost"),
        "{}",
        stderr(&refused)
    );
}

/// A second spawn of a live seat's id is refused before it writes anything, so the live seat's
/// launch script stays as it was.
#[test]
fn a_refused_duplicate_spawn_leaves_the_live_seats_script() {
    let home = scratch("duplicate-script");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane-spawn\nrun = printf pane-run\nring = true\nlist = true\nclose = true\ncapture = true\n",
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
    let first = swarm(
        &home,
        &chair,
        &["spawn", "seat", "coder", "--", "echo", "first"],
    );
    assert!(first.status.success(), "{}", stderr(&first));
    let script = home.join(format!(".swarm/runs/{session}/seat.sh"));
    let before = std::fs::read_to_string(&script).unwrap();
    let second = swarm(
        &home,
        &chair,
        &["spawn", "seat", "coder", "--", "echo", "second"],
    );
    assert!(!second.status.success());
    assert_eq!(std::fs::read_to_string(&script).unwrap(), before);
}
