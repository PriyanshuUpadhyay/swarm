use super::*;
use crate::serve::test_support::*;
use swarm::bus::CLI_DEFAULT_MODEL_FLAG;

fn linked_launch_fixture() -> (Fixture, std::process::Command) {
    let fixture = Fixture::new();
    let home = fixture.app.root.parent().unwrap();
    std::fs::write(
        fixture.app.root.join("adapters/fake.conf"),
        "self = true\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();

    let mut launch = std::process::Command::new(fixture.app.sources.worker_cli.as_ref().unwrap());
    launch
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("SWARM_HOME", home)
        .env("SWARM_ADAPTER", "fake")
        .env("SWARM_SESSION_ID", &fixture.session)
        .env("SWARM_AGENT_ID", "orchestrator")
        .current_dir(home);

    launch.args([
        "launch",
        "orchestrator",
        "chat",
        "--provider",
        "codex",
        CLI_DEFAULT_MODEL_FLAG,
    ]);
    (fixture, launch)
}

#[test]
fn cli_default_launch_omits_the_provider_model_flag() {
    let (fixture, mut launch) = linked_launch_fixture();
    let output = launch.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let script = fixture
        .app
        .root
        .join("runs")
        .join(&fixture.session)
        .join("orchestrator.sh");
    let command = std::fs::read_to_string(script).unwrap();
    assert!(command.contains("'codex'"), "{command}");
    assert!(!command.contains("'--model'"), "{command}");
}

#[test]
fn cli_default_launch_rejects_an_explicit_model() {
    let (_fixture, mut launch) = linked_launch_fixture();
    let output = launch.args(["--model", "gpt-test"]).output().unwrap();
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: swarm"));
}

#[tokio::test]
async fn cli_default_launch_leaves_no_model_to_remember() {
    let (fixture, mut launch) = linked_launch_fixture();
    let output = launch.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    remember_started_model(&fixture.app, &fixture.session).await;
    let db = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    let chair = swarm::store::agents(&db, &fixture.session)
        .unwrap()
        .into_iter()
        .find(|agent| agent.id == "orchestrator")
        .unwrap();
    let saved = choices_store::load(&fixture.app.root, true).unwrap();
    assert_eq!(
        (chair.model, saved.owner.prefs.get("recentModels")),
        (None, None)
    );
}

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
async fn file_carry_keeps_the_error_from_an_unreadable_older_log() {
    for (provider, expected) in [
        ("claude", "transcript log unavailable"),
        ("unknown", "no transcript format"),
    ] {
        let (fixture, mut body) = ended_file_carry();
        let db = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        let leaf = new_linked_test_session(&db, &fixture.session, Some(("claude", "leaf")));
        swarm::store::set_chair_log(&db, &leaf, &fixture.log).unwrap();

        let missing = fixture.dir.join("missing.jsonl");
        db.execute(
            "UPDATE session SET chair_provider = ?1, chair_log = ?2 WHERE id = ?3",
            (provider, missing.to_str().unwrap(), &fixture.session),
        )
        .unwrap();

        body.old_session = leaf;
        let err = prepare_ended_file(&fixture, &body).await.unwrap_err();
        assert_eq!(err.0, StatusCode::CONFLICT);
        assert_eq!(err.1.0["error"], expected);
        assert!(!fixture.app.root.join("handoffs").exists());
    }
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
