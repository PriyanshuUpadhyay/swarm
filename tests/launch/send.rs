use super::*;

#[test]
fn a_send_refuses_an_empty_body_without_storing_or_ringing() {
    let home = scratch("send-empty");
    std::fs::write(
        home.join("screen"),
        include_str!("../fixtures/screens/codex-idle.txt"),
    )
    .unwrap();
    let (session, connection) = seat_session(&home, "codex", "printf ring >> \"$HOME/rings\"");
    for body in ["", " \t\n"] {
        let sent = swarm_input(
            &home,
            &chair(&session),
            &["send", "seat", "ask"],
            Some(body),
        );
        assert!(!sent.status.success(), "blank body was accepted: {body:?}");
        let error = stderr(&sent);
        assert!(error.contains("empty"), "{error}");
        assert!(error.contains("pipe") && error.contains("stdin"), "{error}");
        assert_eq!(error.lines().count(), 1, "{error}");
        assert!(
            swarm::store::messages(&connection, &session, -1)
                .unwrap()
                .is_empty()
        );
        assert!(!home.join("rings").exists());
        let run = home.join(".swarm/runs").join(&session);
        assert!(!run.exists() || std::fs::read_dir(run).unwrap().next().is_none());
    }
}

#[test]
fn a_send_preserves_a_normal_body_and_rings_the_seat() {
    let home = scratch("send-body");
    std::fs::write(
        home.join("screen"),
        include_str!("../fixtures/screens/codex-idle.txt"),
    )
    .unwrap();
    let (session, connection) = seat_session(
        &home,
        "codex",
        "printf ring >> \"$HOME/rings\"; SWARM_AGENT_ID=seat \"$SWARM_EXE\" hook codex UserPromptSubmit < /dev/null",
    );
    let body = "  hello seat\n";
    let sent = swarm_input(
        &home,
        &chair(&session),
        &["send", "seat", "ask"],
        Some(body),
    );
    assert!(sent.status.success(), "{}", stderr(&sent));
    let message = swarm::store::messages(&connection, &session, -1)
        .unwrap()
        .remove(0);
    assert_eq!(
        std::fs::read_to_string(home.join(".swarm").join(message.body_path)).unwrap(),
        body
    );
    assert_eq!(std::fs::read_to_string(home.join("rings")).unwrap(), "ring");
}
