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
        model: Some("gpt-5".into()),
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
async fn idle_guard_reports_a_stored_log_read_failure() {
    let fixture = Fixture::new();
    std::fs::remove_file(&fixture.log).unwrap();
    let chair = json!({"state": "done"});

    let err = require_idle(&fixture.app, &fixture.session, &chair)
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::NOT_FOUND);
    assert_eq!(err.1.0["error"], "transcript log unavailable");
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
