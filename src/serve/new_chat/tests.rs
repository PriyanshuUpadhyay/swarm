//! Tests for server new chat.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn new_chat_route() {
    let fixture = action_fixture();
    let body = json!({"cwd": env!("CARGO_MANIFEST_DIR"), "provider": "codex", "model": "gpt-test", "account": "auto"});
    let response = start_chat(&fixture, body).await.unwrap();
    assert!(uuid::Uuid::parse_str(response["session"].as_str().unwrap()).is_ok());
    assert_eq!(response["workspace"], env!("CARGO_MANIFEST_DIR"));
    // A page whose stream was down reads the done frame again.
    let query = HashMap::from([("id".into(), response["start"].as_str().unwrap().to_owned())]);
    let kept = start::chat_start_done(State(fixture.app.clone()), Query(query)).await;
    assert_eq!(kept.0["done"], response);
    let query = HashMap::from([("id".into(), "unknown".to_owned())]);
    let unknown = start::chat_start_done(State(fixture.app.clone()), Query(query)).await;
    assert_eq!(unknown.0, json!({}));
    let calls = action_calls(&fixture);
    let lines: Vec<_> = calls.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].ends_with("|init"));
    assert!(lines[1].ends_with("|session new lane"));
    assert!(
        lines[2].ends_with(
            "|launch orchestrator chat --provider codex --model gpt-test --account auto"
        )
    );
    assert_eq!(
        call_action(
            &fixture,
            "/api/chat/new",
            json!({"cwd": "/not-in-snapshot"})
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::FORBIDDEN
    );
    for bad in [
        json!({}),
        json!({"cwd": 3}),
        json!({"cwd": "relative"}),
        json!({"cwd": env!("CARGO_MANIFEST_DIR"), "model": "--bad"}),
    ] {
        assert_eq!(
            call_action(&fixture, "/api/chat/new", bad)
                .await
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(action_calls(&fixture), calls);
    let _ = start_chat(&fixture, json!({"cwd": env!("CARGO_MANIFEST_DIR")}))
        .await
        .unwrap();
    assert!(action_calls(&fixture).ends_with("|launch orchestrator chat --account auto\n"));
}

#[tokio::test]
async fn an_archived_git_root_can_start_only_in_a_new_worktree() {
    let fixture = action_fixture();
    let repo = fixture.dir.join("archived-repo");
    std::fs::create_dir(&repo).unwrap();
    let repo = std::fs::canonicalize(repo).unwrap();
    let cwd = repo.to_str().unwrap();

    lifecycle::check_git(cwd, &["init", "-qb", "main"]).unwrap();
    lifecycle::check_git(
        cwd,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--no-gpg-sign",
            "--allow-empty",
            "-qm",
            "base",
        ],
    )
    .unwrap();

    choices_store::update(&fixture.app.root, None, |owner, _| {
        owner.project_paths.push(cwd.into());
        owner.archived.insert(cwd.into());
        Ok(())
    })
    .unwrap();

    let here = call_action(
        &fixture,
        "/api/chat/new",
        json!({"cwd":cwd,"place":{"kind":"here"}}),
    )
    .await
    .unwrap_err();
    assert_eq!(here.0, StatusCode::CONFLICT);
    let files = menu::new_chat_files(
        State(fixture.app.clone()),
        Query(serde_json::from_value(json!({"cwd": cwd})).unwrap()),
    )
    .await
    .unwrap_err();
    assert_eq!(files.0, StatusCode::CONFLICT);
    assert_eq!(
        files.1.0["error"],
        "Files and commands are not listed for an archived workspace."
    );

    let started = start_chat(&fixture, json!({"cwd":cwd,"place":{"kind":"new"}}))
        .await
        .unwrap();
    assert_ne!(started["workspace"], cwd);
    assert!(Path::new(started["workspace"].as_str().unwrap()).is_dir());
}

#[tokio::test]
async fn start_status_keeps_the_first_send_failure() {
    let fixture = action_fixture();
    let session = uuid::Uuid::now_v7();

    std::fs::write(
        &fixture.app.cli,
        format!(
            r#"#!/bin/sh
case "$1 $2" in
  'session new') printf '%s\n' '{session}' ;;
  'launch orchestrator') printf 'fixture-pane\n' ;;
  type\ *) printf 'The chair is not ready.\n' >&2; exit 1 ;;
esac
"#
        ),
    )
    .unwrap();

    let done = start_chat(
        &fixture,
        json!({"cwd": env!("CARGO_MANIFEST_DIR"), "prompt": "Fix it"}),
    )
    .await
    .unwrap();

    let query = HashMap::from([("id".into(), done["start"].as_str().unwrap().to_owned())]);
    let kept = start::chat_start_done(State(fixture.app.clone()), Query(query)).await;
    assert_eq!(kept.0["done"], done);
    assert_eq!(done["session"], session.to_string());
    assert!(
        done["sendError"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
}

#[tokio::test]
async fn new_chat_commands_lists_a_provider_in_the_folder_before_a_chat_exists() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
printf '%s\n' "$PWD|$*" >> "$SWARM_HOME/actions"
printf '{"commands":[{"insert":"/swarm:plan"}],"note":null}\n'
"#,
    )
    .unwrap();
    let read = |cwd: &str| {
        new_chat_commands(
            State(fixture.app.clone()),
            Query(serde_json::from_value(json!({"cwd": cwd, "provider": "claude"})).unwrap()),
        )
    };
    let list = read(env!("CARGO_MANIFEST_DIR")).await.unwrap().0;
    assert_eq!(list["commands"][0]["insert"], "/swarm:plan");
    assert_eq!(
        action_calls(&fixture),
        format!(
            "{}|commands --provider claude --json\n",
            env!("CARGO_MANIFEST_DIR")
        )
    );
    // A folder outside the workspace tree is not read.
    assert_eq!(
        read("/not-in-snapshot").await.unwrap_err().0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn resume_chat_route_launches_the_chair_with_resume_in_its_chat() {
    let fixture = action_fixture();
    let body = json!({"session": fixture.session});
    let response = call_action(&fixture, "/api/chat/resume", body)
        .await
        .unwrap()
        .0;
    assert_eq!(
        (&response["session"], &response["pane"]),
        (&json!(fixture.session), &json!("fixture-pane"))
    );
    let calls = action_calls(&fixture);
    assert!(
        calls.starts_with(&format!("tmux-swarm|{}|", fixture.session))
            && calls.ends_with("|launch orchestrator chat --resume\n"),
        "{calls}"
    );
    let unknown = json!({"session": uuid::Uuid::now_v7().to_string()});
    let refused = call_action(&fixture, "/api/chat/resume", unknown).await;
    assert_eq!(refused.unwrap_err().0, StatusCode::NOT_FOUND);
}
