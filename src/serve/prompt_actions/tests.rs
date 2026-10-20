//! Tests for server prompt actions.

use super::*;
use crate::serve::test_support::*;
#[tokio::test]
async fn pull_back_route_decides_records_then_clears_only_confirmed_boxes() {
    for (after, outcome, text, clears) in [
        (
            queue_log("popAll", "first") + &queue_log("popAll", "second"),
            "pulled",
            Some("first\nsecond"),
            5,
        ),
        (
            queue_log("popAll", "first") + &queue_log("remove", "second"),
            "pulled",
            Some("first"),
            3,
        ),
        (
            queue_log("dequeue", "") + &queue_log("dequeue", ""),
            "alreadySent",
            None,
            3,
        ),
        (String::new(), "unconfirmed", None, 0),
    ] {
        let fixture = pull_back_fixture(
            &(queue_log("enqueue", "first") + &queue_log("enqueue", "second")),
            &after,
        );
        let Json(result) = call_action(
            &fixture,
            "/api/chat/pull-back",
            json!({"session":fixture.session,"agent":"chair"}),
        )
        .await
        .unwrap();
        assert_eq!(result, json!({"ok":true,"outcome":outcome,"text":text}));
        let calls = action_calls(&fixture);
        assert_eq!(
            calls.lines().filter(|line| line.ends_with(" Up")).count(),
            1
        );
        assert_eq!(
            calls.lines().filter(|line| line.ends_with(" C-u")).count(),
            clears
        );
    }
}
#[tokio::test]
async fn a_claude_stop_clears_the_prompt_that_claude_puts_back_in_its_box() {
    let user = |text: &str| {
        json!({"type":"user","uuid":text,"message":{"role":"user","content":text}}).to_string()
            + "\n"
    };
    let fixture = pull_back_fixture(&(user("first") + &user("two\nlines")), "");
    let _ = call_action(
        &fixture,
        "/api/chat/interrupt",
        json!({"session":fixture.session,"agent":"chair"}),
    )
    .await
    .unwrap();
    let calls = action_calls(&fixture);
    let calls: Vec<_> = calls.lines().collect();
    assert!(calls[0].ends_with("interrupt orchestrator"), "{calls:?}");
    assert_eq!(
        calls[1..]
            .iter()
            .filter(|line| line.ends_with(" C-u"))
            .count(),
        5
    );

    // A first turn stopped before its log is stored still stops.
    std::fs::remove_file(&fixture.log).unwrap();
    std::fs::remove_file(fixture.app.root.parent().unwrap().join("actions")).unwrap();
    let _ = call_action(
        &fixture,
        "/api/chat/interrupt",
        json!({"session":fixture.session,"agent":"chair"}),
    )
    .await
    .unwrap();
    assert!(action_calls(&fixture).starts_with("interrupt orchestrator"));
}
#[tokio::test]
async fn pull_back_route_refuses_agent_queue_and_unsupported_targets_without_up() {
    for notification in ["<task-notification>needed", "<agent-message>needed"] {
        let fixture = pull_back_fixture(
            &(queue_log("enqueue", notification) + &queue_log("enqueue", "owner")),
            "",
        );
        assert_eq!(
            call_action(
                &fixture,
                "/api/chat/pull-back",
                json!({"session":fixture.session,"agent":"chair"})
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::CONFLICT
        );
        assert!(action_calls(&fixture).is_empty());
    }
    for (provider, adapter) in [
        ("claude", "herdr"),
        ("codex", "tmux"),
        ("claude", "fixture"),
    ] {
        let fixture = pull_back_fixture(&queue_log("enqueue", "owner"), "");
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET chair_provider=?1,adapter=?2 WHERE id=?3",
                (provider, adapter, &fixture.session),
            )
            .unwrap();
        assert_eq!(
            call_action(
                &fixture,
                "/api/chat/pull-back",
                json!({"session":fixture.session,"agent":"chair"})
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::CONFLICT
        );
        assert!(action_calls(&fixture).is_empty());
    }
    let fixture = pull_back_fixture(&queue_log("enqueue", "<agent-message>needed"), "");
    let Json(result) = call_action(
        &fixture,
        "/api/chat/pull-back",
        json!({"session":fixture.session,"agent":"chair"}),
    )
    .await
    .unwrap();
    assert_eq!(
        result,
        json!({"ok":true,"outcome":"alreadySent","text":null})
    );
    assert!(action_calls(&fixture).is_empty());
}
