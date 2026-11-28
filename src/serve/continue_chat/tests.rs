use super::*;
use crate::serve::test_support::*;
use swarm::bus::CLI_DEFAULT_MODEL_FLAG;

fn launch_fixture() -> Fixture {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.app.root.join("adapters/fake.conf"),
        "self = true\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    fixture
}

fn test_launch(fixture: &Fixture, session: &str) -> std::process::Command {
    let home = fixture.app.root.parent().unwrap();
    let mut launch = std::process::Command::new(fixture.app.sources.worker_cli.as_ref().unwrap());
    launch
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("SWARM_HOME", home)
        .env("SWARM_ADAPTER", "fake")
        .env("SWARM_SESSION_ID", session)
        .env("SWARM_AGENT_ID", "orchestrator")
        .current_dir(home)
        .args(["launch", "orchestrator", "chat"]);
    launch
}

fn linked_launch_fixture() -> (Fixture, std::process::Command) {
    let fixture = launch_fixture();
    let mut launch = test_launch(&fixture, &fixture.session);
    launch.args(["--provider", "codex", CLI_DEFAULT_MODEL_FLAG]);
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

fn ended_default_pi_chair() -> (Fixture, rusqlite::Connection) {
    let fixture = launch_fixture();
    let db = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    let mut launch = test_launch(&fixture, &fixture.session);
    run_test_launch(launch.args([
        "--provider",
        "pi",
        CLI_DEFAULT_MODEL_FLAG,
        "--effort",
        "low",
    ]));
    swarm::store::clear_pane(&db, &fixture.session, "orchestrator").unwrap();
    (fixture, db)
}

fn run_test_launch(launch: &mut std::process::Command) {
    let output = launch.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_pi_cli_default(fixture: &Fixture, db: &rusqlite::Connection, session: &str) -> String {
    let script = fixture
        .app
        .root
        .join("runs")
        .join(session)
        .join("orchestrator.sh");
    let command = std::fs::read_to_string(script).unwrap();
    let chair = swarm::store::agents(db, session)
        .unwrap()
        .into_iter()
        .find(|agent| agent.id == "orchestrator")
        .unwrap();

    assert!(command.contains("'pi' '--thinking' 'low'"), "{command}");
    assert!(!command.contains("'--model'"), "{command}");
    assert_eq!(chair.model, None);
    command
}

#[test]
fn resume_a_pi_chair_without_a_model_uses_the_cli_default() {
    let (fixture, db) = ended_default_pi_chair();
    run_test_launch(test_launch(&fixture, &fixture.session).arg("--resume"));

    let command = assert_pi_cli_default(&fixture, &db, &fixture.session);
    assert!(
        command.contains(&format!(
            "'--session-id' '{}-orchestrator'",
            fixture.session
        )),
        "{command}"
    );
}

#[test]
fn branch_from_a_pi_chair_without_a_model_uses_the_cli_default() {
    let (fixture, db) = ended_default_pi_chair();
    let branch = new_test_session(&db, None);
    swarm::store::branch_session(&db, &branch, &fixture.session, None).unwrap();
    run_test_launch(test_launch(&fixture, &branch).args(["--provider", "pi", "--effort", "low"]));

    assert_pi_cli_default(&fixture, &db, &branch);
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
