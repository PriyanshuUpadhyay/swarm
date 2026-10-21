// Tests for focus.
use super::*;

#[test]
fn focus_uses_the_stored_adapter_and_pane_outside_a_session() {
    let home = scratch("focus-host");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for host in ["herdr", "tmux"] {
        std::fs::write(
            home.join(format!(".swarm/adapters/{host}.conf")),
            "client = true\n",
        )
        .unwrap();
        let executable = bin.join(host);
        std::fs::write(
            &executable,
            "#!/bin/sh\nfor arg; do printf '<%s>' \"$arg\"; done >> \"$HOME/focus\"\necho >> \"$HOME/focus\"\n",
        )
        .unwrap();
        std::fs::set_permissions(
            executable,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
    }
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    for (adapter, pane, expected) in [
        ("herdr", "wK:p2", "<agent><focus><wK:p2>\n"),
        (
            "tmux",
            "%2",
            "<select-window><-t><%2>\n<select-pane><-t><%2>\n",
        ),
    ] {
        let created = swarm(&home, &[], &["session", "new", "lane"], "");
        assert!(created.status.success(), "{created:?}");
        let session = stdout(&created).trim().to_string();
        let added = swarm(
            &home,
            &[("SWARM_SESSION_ID", &session)],
            &["agent", "add", "coder", "coder"],
            "",
        );
        assert!(added.status.success(), "{added:?}");
        swarm::store::set_adapter(&connection, &session, adapter).unwrap();
        swarm::store::set_pane(&connection, &session, "coder", pane).unwrap();
        let focused = swarm(
            &home,
            &[("PATH", &path), ("SWARM_ADAPTER", "wrong")],
            &["focus", &session, "coder"],
            "",
        );
        assert!(focused.status.success(), "{adapter}: {focused:?}");
        assert_eq!(
            std::fs::read_to_string(home.join("focus")).unwrap(),
            expected
        );
        std::fs::remove_file(home.join("focus")).unwrap();
    }
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn focus_names_missing_panes_adapters_and_verbs() {
    let home = scratch("focus-errors");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let session = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
    swarm::store::add_agent(&connection, &session, "coder", "coder").unwrap();
    let focused = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert!(!focused.status.success(), "{focused:?}");
    assert_eq!(
        String::from_utf8_lossy(&focused.stderr),
        "swarm: session has no adapter\n"
    );
    // tmux-swarm opens Swarm.app (ADR 0102), so a test cannot run it without a window.
    swarm::store::set_adapter(&connection, &session, "superset").unwrap();
    let missing = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert!(!missing.status.success(), "{missing:?}");
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        "swarm: superset has no focus\n"
    );
    swarm::store::set_pane(&connection, &session, "coder", "%2").unwrap();
    let missing = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert!(!missing.status.success(), "{missing:?}");
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        "swarm: superset has no focus\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn focus_retries_the_chair_and_raises_the_clients_app() {
    let home = scratch("focus-chair");
    assert!(swarm(&home, &[], &["init"], "").status.success());
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let session =
        swarm::store::create_session(&connection, "lane", &home, None, Some("herdr")).unwrap();
    for (agent, role, pane) in [
        ("coder", "coder", "closed"),
        ("chair", "orchestrator", "chair-pane"),
    ] {
        swarm::store::add_agent(&connection, &session, agent, role).unwrap();
        swarm::store::set_pane(&connection, &session, agent, pane).unwrap();
    }
    std::fs::write(
        home.join(".swarm/adapters/herdr.conf"),
        "client = printf 101\n",
    )
    .unwrap();
    for (name, script) in [
        (
            "herdr",
            "#!/bin/sh\necho \"$3\" >> \"$HOME/focus\"\ntest \"$3\" = chair-pane\n",
        ),
        (
            "ps",
            "#!/bin/sh\ncase \"$2:$4\" in comm=:101) echo herdr;; ppid=:101) echo 102;; comm=:102) echo zsh;; ppid=:102) echo 103;; comm=:103) echo login;; ppid=:103) echo 104;; comm=:104) echo '/Applications/Terminal Host.app/Contents/MacOS/ghostty';; *) exit 1;; esac\n",
        ),
        (
            "open",
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$HOME/raised\"\n",
        ),
    ] {
        let path = home.join("bin").join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }
    let focused = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert!(focused.status.success(), "{focused:?}");
    assert_eq!(
        std::fs::read_to_string(home.join("focus")).unwrap(),
        "closed\nchair-pane\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("raised")).unwrap(),
        "/Applications/Terminal Host.app\n"
    );
    swarm::store::clear_pane(&connection, &session, "coder").unwrap();
    assert!(
        swarm(&home, &[], &["focus", &session, "coder"], "")
            .status
            .success()
    );
    std::fs::remove_file(home.join("raised")).unwrap();
    std::fs::write(
        home.join("bin/ps"),
        "#!/bin/sh\ncase \"$2\" in comm=) echo herdr;; ppid=) echo 1;; esac\n",
    )
    .unwrap();
    assert!(
        swarm(&home, &[], &["focus", &session, "coder"], "")
            .status
            .success()
    );
    assert!(!home.join("raised").exists());
    swarm::store::clear_pane(&connection, &session, "chair").unwrap();
    let gone = swarm(&home, &[], &["focus", &session, "coder"], "");
    assert_eq!(gone.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&gone.stderr),
        "swarm: pane closed\n"
    );
    std::fs::remove_dir_all(home).unwrap();
}
