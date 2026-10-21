fn handoff_fixture() -> (Fixture, ContinueChat) {
    use std::os::unix::fs::PermissionsExt;
    let fixture = action_fixture();
    let home = fixture.app.root.parent().unwrap();
    let new_id = uuid::Uuid::now_v7().to_string();
    std::fs::write(home.join("old-id"), &fixture.session).unwrap();
    std::fs::write(home.join("new-id"), &new_id).unwrap();
    std::fs::write(home.join("ready-list.json"), json!({"sessions":[{"id":new_id,"chair_provider":"codex","chair_id":"ready","chair_log":null}]}).to_string()).unwrap();
    std::fs::write(home.join("summary-turn.jsonl"), [
        json!({"type":"user","uuid":"summary-request","message":{"role":"user","content":handoff::REQUEST}}),
        json!({"type":"assistant","uuid":"summary-answer","message":{"role":"assistant","content":[{"type":"text","text":"Keep the user's files"}]}}),
        json!({"type":"system","subtype":"turn_duration","uuid":"summary-end","durationMs":1}),
    ].iter().map(|e| format!("{e}\n")).collect::<String>()).unwrap();
    std::fs::write(&fixture.app.cli, r#"#!/bin/sh
printf '%s\n' "$SWARM_SESSION_ID|$*" >> "$SWARM_HOME/handoff-actions"
case "$1 $2" in
  'agents --json')
    if [ -f "$SWARM_HOME/source-alive" ]; then printf '%s\n' '{"agents":[{"id":"orchestrator","alive":true}]}'
    else printf '%s\n' '{"agents":[]}'; fi ;;
  'session new') cat "$SWARM_HOME/new-id" ;;
  'launch orchestrator')
    while [ -f "$SWARM_HOME/block-launch" ]; do /bin/sleep .01; done
    printf 'fixture-pane\n' ;;
  'sessions --json') cat "$SWARM_HOME/ready-list.json" ;;
  'type orchestrator')
    cat > "$SWARM_HOME/handoff-text"
    if [ "$SWARM_SESSION_ID" = "$(cat "$SWARM_HOME/old-id")" ]; then
      if [ ! -f "$SWARM_HOME/no-summary" ]; then cat "$SWARM_HOME/summary-turn.jsonl" >> "$SWARM_HOME/../chair.jsonl"; fi
    elif [ -f "$SWARM_HOME/fail-delivery" ]; then printf 'delivery failed\n' >&2; exit 1; fi ;;
esac
"#).unwrap();
    std::fs::set_permissions(&fixture.app.cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .insert("progress".into(), client(&[]));
    let body = ContinueChat {
        old_session: fixture.session.clone(),
        provider: "codex".into(),
        model: "gpt-6-sol".into(),
        account: None,
        effort: None,
        carry: Carry::Summary,
    };
    (fixture, body)
}
fn handoff_phases(fixture: &Fixture) -> Vec<String> {
    fixture.app.hub.lock().unwrap().streams["progress"]
        .queue
        .iter()
        .filter(|f| f.name == "handoff")
        .map(|f| {
            serde_json::from_str::<Value>(&f.data).unwrap()["phase"]
                .as_str()
                .unwrap()
                .into()
        })
        .collect()
}
async fn handoff_test_run(fixture: &Fixture, body: ContinueChat) -> Result<Value, ApiError> {
    register_continue(&fixture.app, &body)?;
    execute_continue_timed(
        &fixture.app,
        body,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_millis(10),
    )
    .await
}
#[tokio::test]
async fn handoff_cancel_before_launch() {
    let (fixture, body) = handoff_fixture();
    register_continue(&fixture.app, &body).unwrap();
    let _ = cancel_continue(
        State(fixture.app.clone()),
        Ok(Json(CancelContinue {
            old_session: fixture.session.clone(),
        })),
    )
    .await
    .unwrap();
    let err = execute_continue(&fixture.app, body).await.unwrap_err();
    assert_eq!(err.1.0["error"], "model switch cancelled");
    assert!(!fixture.dir.join("home/handoff-actions").exists());
    assert_eq!(handoff_phases(&fixture), ["cancelled"]);
    assert!(fixture.app.handoffs.lock().unwrap().is_empty());
}
#[tokio::test]
async fn handoff_active_source() {
    let (fixture, body) = handoff_fixture();
    std::fs::write(fixture.dir.join("home/source-alive"), "").unwrap();
    std::fs::OpenOptions::new().append(true).open(&fixture.log).unwrap().write_all(b"{\"type\":\"user\",\"uuid\":\"active\",\"message\":{\"role\":\"user\",\"content\":\"Summary still running\"}}\n").unwrap();
    let err = handoff_test_run(&fixture, body).await.unwrap_err();
    assert!(
        err.1.0["error"]
            .as_str()
            .unwrap()
            .contains("Wait for the current reply")
    );
    let calls = std::fs::read_to_string(fixture.dir.join("home/handoff-actions")).unwrap();
    assert_eq!(calls.lines().count(), 1);
    assert!(calls.contains("agents --json"));
    assert!(!calls.contains("interrupt"));
}
#[tokio::test]
async fn handoff_successful_handoff() {
    let (fixture, body) = handoff_fixture();
    let result = handoff_test_run(&fixture, body).await.unwrap();
    assert_eq!(
        result["session"].as_str().unwrap(),
        std::fs::read_to_string(fixture.dir.join("home/new-id")).unwrap()
    );
    assert_eq!(
        handoff_phases(&fixture),
        [
            "preparing",
            "starting",
            "waiting",
            "delivering",
            "completed"
        ]
    );
    let calls = std::fs::read_to_string(fixture.dir.join("home/handoff-actions")).unwrap();
    let calls: Vec<_> = calls.lines().collect();
    assert!(calls[calls.len() - 2].ends_with("type orchestrator"));
    assert!(calls[calls.len() - 1].contains("session continue"));
    let text = std::fs::read_to_string(fixture.dir.join("home/handoff-text")).unwrap();
    assert!(text.contains("These are recent messages"));
    assert!(text.contains("Check the worker inbox"));
    assert!(fixture.app.handoffs.lock().unwrap().is_empty());
}
#[tokio::test]
async fn handoff_delivery_failure() {
    let (fixture, body) = handoff_fixture();
    std::fs::write(fixture.dir.join("home/fail-delivery"), "").unwrap();
    assert_eq!(
        handoff_test_run(&fixture, body).await.unwrap_err().1.0["error"],
        "delivery failed"
    );
    assert!(
        !std::fs::read_to_string(fixture.dir.join("home/handoff-actions"))
            .unwrap()
            .contains("session continue")
    );
    assert_eq!(handoff_phases(&fixture).last().unwrap(), "failed");
}
#[tokio::test]
async fn handoff_live_summary() {
    let (fixture, body) = handoff_fixture();
    std::fs::write(fixture.dir.join("home/source-alive"), "").unwrap();
    handoff_test_run(&fixture, body).await.unwrap();
    assert_eq!(
        handoff_phases(&fixture),
        [
            "preparing",
            "summarizing",
            "starting",
            "waiting",
            "delivering",
            "completed"
        ]
    );
    let text = std::fs::read_to_string(fixture.dir.join("home/handoff-text")).unwrap();
    assert!(text.contains("compact handoff summary:"));
    assert!(text.contains("Keep the user's files"));
}
#[tokio::test]
async fn handoff_summary_timeout_falls_back_and_ready_timeout_leaves_separate() {
    for ready in [true, false] {
        let (fixture, body) = handoff_fixture();
        let home = fixture.dir.join("home");
        std::fs::write(home.join("source-alive"), "").unwrap();
        std::fs::write(home.join("no-summary"), "").unwrap();
        if !ready {
            std::fs::write(home.join("ready-list.json"), "{\"sessions\":[]}").unwrap();
        }
        register_continue(&fixture.app, &body).unwrap();
        let result = execute_continue_timed(
            &fixture.app,
            body,
            Duration::from_millis(50),
            Duration::from_millis(70),
            Duration::from_millis(10),
        )
        .await;
        if ready {
            result.unwrap();
            assert!(
                std::fs::read_to_string(home.join("handoff-text"))
                    .unwrap()
                    .contains("These are recent messages")
            );
        } else {
            assert_eq!(result.unwrap_err().0, StatusCode::GATEWAY_TIMEOUT);
            assert!(
                !std::fs::read_to_string(home.join("handoff-actions"))
                    .unwrap()
                    .contains("session continue")
            );
        }
    }
}
#[tokio::test]
async fn handoff_cancel_during_summary_and_refused_after_starting() {
    for late in [false, true] {
        let (fixture, body) = handoff_fixture();
        let home = fixture.dir.join("home");
        std::fs::write(
            home.join(if late { "block-launch" } else { "source-alive" }),
            "",
        )
        .unwrap();
        std::fs::write(home.join("no-summary"), "").unwrap();
        register_continue(&fixture.app, &body).unwrap();
        let app = fixture.app.clone();
        let task = tokio::spawn(async move { execute_continue(&app, body).await });
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if handoff_phases(&fixture)
                    .contains(&if late { "starting" } else { "summarizing" }.into())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let result = cancel_continue(
            State(fixture.app.clone()),
            Ok(Json(CancelContinue {
                old_session: fixture.session.clone(),
            })),
        )
        .await;
        if late {
            assert_eq!(result.unwrap_err().0, StatusCode::CONFLICT);
            std::fs::remove_file(home.join("block-launch")).unwrap();
            assert!(task.await.unwrap().is_ok());
        } else {
            let _ = result.unwrap();
            assert_eq!(
                task.await.unwrap().unwrap_err().1.0["error"],
                "model switch cancelled"
            );
            assert!(
                !std::fs::read_to_string(home.join("handoff-actions"))
                    .unwrap()
                    .contains("session new")
            );
        }
    }
}
#[tokio::test]
async fn handoff_rejects_duplicate_unknown_and_bad_launch_before_cli() {
    let (fixture, mut body) = handoff_fixture();
    // AGY and pi switch too (ADR 0108); only an unknown CLI is refused.
    body.provider = "gemini".into();
    assert_eq!(
        register_continue(&fixture.app, &body).unwrap_err().0,
        StatusCode::BAD_REQUEST
    );
    body.provider = "codex".into();
    body.model = "--option".into();
    assert_eq!(
        register_continue(&fixture.app, &body).unwrap_err().0,
        StatusCode::BAD_REQUEST
    );
    body.model = "gpt-6-sol".into();
    body.old_session = "unknown".into();
    assert_eq!(
        register_continue(&fixture.app, &body).unwrap_err().0,
        StatusCode::NOT_FOUND
    );
    body.old_session = fixture.session.clone();
    register_continue(&fixture.app, &body).unwrap();
    assert_eq!(
        register_continue(&fixture.app, &body).unwrap_err().0,
        StatusCode::CONFLICT
    );
    assert!(!fixture.dir.join("home/handoff-actions").exists());
}
#[tokio::test]
async fn menu_reads_forward_cli_json_and_errors() {
    let fixture = action_fixture();
    let script = r#"#!/bin/sh
printf '%s\n' "$*" >> "$SWARM_HOME/menu-calls"
if [ -f "$SWARM_HOME/menu-failure" ]; then printf 'menu failed\n' >&2; exit 1; fi
if [ -f "$SWARM_HOME/menu-invalid" ]; then printf 'bad-json'; exit 0; fi
cat "$SWARM_HOME/menu.json"
"#;
    std::fs::write(&fixture.app.cli, script).unwrap();
    let home = fixture.dir.join("home");
    for (route, provider, payload, args) in [
        (
            "roles",
            None,
            json!({"revision":"roles-r","profiles":[{"name":"chat","runners":[]}]}),
            "roles --json",
        ),
        (
            "providers",
            None,
            json!({"providers":[{"id":"codex","name":"Codex"}]}),
            "providers --json",
        ),
        (
            "models",
            Some("codex"),
            json!({"provider":"codex","models":[{"id":"gpt-6-sol"}]}),
            "models --provider codex --json",
        ),
        (
            "models",
            Some("pi"),
            json!({"provider":"pi","models":[{"id":"anthropic/claude-haiku-5-5"}]}),
            "models --provider pi --json",
        ),
        (
            "accounts",
            Some("claude"),
            json!({"revision":"accounts-r","accounts":[{"name":"default"}]}),
            "accounts --provider claude --json",
        ),
    ] {
        std::fs::write(home.join("menu.json"), payload.to_string()).unwrap();
        assert_eq!(
            menu_read(
                State(fixture.app.clone()),
                format!("/api/{route}").parse().unwrap(),
                Query(MenuQuery {
                    provider: provider.map(str::to_owned)
                })
            )
            .await
            .unwrap()
            .0,
            payload
        );
        assert_eq!(
            std::fs::read_to_string(home.join("menu-calls"))
                .unwrap()
                .lines()
                .last(),
            Some(args)
        );
    }
    for provider in [None, Some("--bad"), Some("unknown")] {
        assert_eq!(
            menu_read(
                State(fixture.app.clone()),
                "/api/models".parse().unwrap(),
                Query(MenuQuery {
                    provider: provider.map(str::to_owned)
                })
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    std::fs::write(home.join("menu-failure"), "").unwrap();
    assert_eq!(
        menu_read(
            State(fixture.app.clone()),
            "/api/roles".parse().unwrap(),
            Query(MenuQuery { provider: None })
        )
        .await
        .unwrap_err()
        .1
        .0["error"],
        "menu failed"
    );
    std::fs::remove_file(home.join("menu-failure")).unwrap();
    std::fs::write(home.join("menu-invalid"), "").unwrap();
    assert_eq!(
        menu_read(
            State(fixture.app.clone()),
            "/api/roles".parse().unwrap(),
            Query(MenuQuery { provider: None })
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::BAD_GATEWAY
    );
}
