//! Tests for server action execute.

use super::*;
use crate::serve::test_support::*;
#[tokio::test]
async fn send_cleans_claude_only_and_refuses_empty_cleaned_text() {
    let fixture = action_fixture();
    let text = "soft\u{ad}hyphen a\u{200c}b c\u{200b}d e\u{85}f 🏴\u{e0067}\u{e007f}";
    for provider in ["claude", "codex", "agy"] {
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET chair_provider = ?1 WHERE id = ?2",
                (provider, &fixture.session),
            )
            .unwrap();
        let _ = call_action(
            &fixture,
            "/api/chat/send",
            json!({"session":fixture.session,"agent":"chair","text":text}),
        )
        .await
        .unwrap();
        let actual =
            std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap();
        assert_eq!(
            actual,
            if provider == "claude" {
                claude_text::cleaned(text)
            } else {
                text.into()
            }
        );
    }
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET chair_provider = 'claude' WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    let before = action_calls(&fixture);
    assert_eq!(
        call_action(
            &fixture,
            "/api/chat/send",
            json!({"session":fixture.session,"agent":"chair","text":"\u{200b}"})
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(action_calls(&fixture), before);
    // A child provider has its own rule, independent of the chair.
    connection
        .execute(
            "UPDATE agent SET provider = 'codex' WHERE session_id = ?1 AND id = 'worker'",
            [&fixture.session],
        )
        .unwrap();
    let _ = call_action(
        &fixture,
        "/api/chat/send",
        json!({"session":fixture.session,"agent":"worker","text":text}),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap(),
        text
    );
}
#[tokio::test]
async fn send_with_unknown_provider_preserves_text() {
    let fixture = action_fixture();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET chair_provider = NULL WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    let text = "a\u{200b}b";
    let _ = call_action(
        &fixture,
        "/api/chat/send",
        json!({"session":fixture.session,"agent":"chair","text":text}),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap(),
        text
    );
}
