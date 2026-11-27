use super::*;
use crate::serve::test_support::*;

fn ended_file_carry() -> (Fixture, ContinueChat) {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        "#!/bin/sh\nprintf '%s\\n' '{\"agents\":[]}'\n",
    )
    .unwrap();

    let body = ContinueChat {
        old_session: fixture.session.clone(),
        provider: "codex".into(),
        model: "gpt-5".into(),
        account: None,
        effort: None,
        carry: Carry::File,
    };
    (fixture, body)
}

async fn prepare_ended_file(
    fixture: &Fixture,
    body: &ContinueChat,
) -> Result<Option<String>, ApiError> {
    register_continue(&fixture.app, body).unwrap();
    let call = current_source(&fixture.app, &body.old_session).unwrap();
    prepare_carry(&fixture.app, body, &call, Duration::ZERO, Duration::ZERO).await
}

#[tokio::test]
async fn file_carry_refuses_a_link_whose_stored_log_is_gone() {
    let (fixture, mut body) = ended_file_carry();
    let db = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    let leaf = swarm::store::create_session(
        &db,
        "lane",
        Path::new(env!("CARGO_MANIFEST_DIR")),
        Some(("pi", "leaf")),
        Some("tmux-swarm"),
    )
    .unwrap();
    swarm::store::continue_session(&db, &leaf, &fixture.session).unwrap();
    db.execute(
        "UPDATE session SET chair_log = ?1 WHERE id = ?2",
        [fixture.dir.join("missing.jsonl").to_str().unwrap(), &leaf],
    )
    .unwrap();

    body.old_session = leaf;
    let err = prepare_ended_file(&fixture, &body).await.unwrap_err();
    assert_eq!(err.0, StatusCode::CONFLICT);
    assert!(!fixture.app.root.join("handoffs").exists());
}

#[tokio::test]
async fn file_carry_refuses_an_oversized_chat_before_writing_or_launching() {
    use std::io::Write;
    let (fixture, body) = ended_file_carry();
    let mut log = std::fs::File::create(&fixture.log).unwrap();
    let content = "x".repeat(20_000);
    for i in 0..850 {
        writeln!(
            log,
            "{}",
            json!({"type":"user","uuid":i.to_string(),"message":{"role":"user","content":content}})
        )
        .unwrap();
    }

    let err = prepare_ended_file(&fixture, &body).await.unwrap_err();
    assert_eq!(err.0, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!fixture.app.root.join("handoffs").exists());
}

#[tokio::test]
async fn ended_file_limit_only_offers_no_history() {
    let (fixture, body) = ended_file_carry();
    std::fs::File::create(&fixture.log)
        .unwrap()
        .set_len(ROWS_FIND_LIMIT as u64 + 1)
        .unwrap();

    let err = prepare_ended_file(&fixture, &body).await.unwrap_err();
    assert_eq!(err.0, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        err.1.0["error"],
        "This chat is too large to carry as a file. Choose No history in Settings."
    );
}

#[tokio::test]
async fn a_switch_of_a_chat_with_no_adapter_reads_its_chair_on_the_apps_adapter() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = action_fixture();
    // An imported chat has no adapter until its first pane.
    swarm::store::open(&fixture.app.root.join("swarm.db"))
        .unwrap()
        .execute(
            "UPDATE session SET adapter = NULL WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    let cli = &fixture.app.cli;
    std::fs::write(
        cli,
        "#!/bin/sh\nprintf '%s\\n' \"${SWARM_ADAPTER-}|$*\" >> \"$SWARM_HOME/actions\"\n\
         if [ \"$1\" = agents ]; then printf '%s\\n' '{\"agents\":[{\"id\":\"orchestrator\"}]}'; fi\n",
    )
    .unwrap();
    std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let body = || -> ContinueChat {
        serde_json::from_value(json!({
            "old_session": fixture.session,
            "provider": "claude",
            "model": "opus",
            "carry": "resume",
        }))
        .unwrap()
    };
    register_continue(&fixture.app, &body()).unwrap();
    // The switch goes on to launch after that read and its result is dropped; only the first read matters here.
    let _ = execute_continue(&fixture.app, body()).await;
    assert_eq!(
        action_calls(&fixture).lines().next(),
        Some("tmux-swarm|agents --json")
    );
}
