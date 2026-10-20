//! Tests for server new chat.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn new_chat_route() {
    let fixture = action_fixture();
    let body = json!({"cwd": env!("CARGO_MANIFEST_DIR"), "provider": "codex", "model": "gpt-test", "account": "auto"});
    let response = call_action(&fixture, "/api/chat/new", body)
        .await
        .unwrap()
        .0;
    assert_eq!(response["ok"], true);
    assert!(uuid::Uuid::parse_str(response["session"].as_str().unwrap()).is_ok());
    assert_eq!(response["agent"], "orchestrator");
    assert_eq!(response["pane"], "fixture-pane");
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
    let _ = call_action(
        &fixture,
        "/api/chat/new",
        json!({"cwd": env!("CARGO_MANIFEST_DIR")}),
    )
    .await
    .unwrap();
    assert!(action_calls(&fixture).ends_with("|launch orchestrator chat --account auto\n"));
}
