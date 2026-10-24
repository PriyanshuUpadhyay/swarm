//! Tests for server actions.

use super::*;
use crate::serve::test_support::*;

macro_rules! target_route_test {
    ($name:ident, $route:literal, $extra:expr, $args:literal) => {
        #[tokio::test]
        async fn $name() {
            let fixture = action_fixture();
            let mut body = json!({"session": fixture.session, "agent": "chair"});
            for (key, value) in $extra.as_object().unwrap() { body[key] = value.clone(); }
            let before = fixture.app.hub.lock().unwrap().version;
            assert_eq!(call_action(&fixture, $route, body.clone()).await.unwrap().0, json!({"ok": true}));
            assert!(fixture.app.hub.lock().unwrap().version > before);
            let calls = action_calls(&fixture);
            let expected = format!("tmux-swarm|{}|orchestrator|{}|{}", fixture.session,
                env!("CARGO_MANIFEST_DIR"), $args);
            assert!(calls.contains(&expected), "{calls}");
            body["session"] = json!("unknown");
            assert_eq!(call_action(&fixture, $route, body.clone()).await.unwrap_err().0, StatusCode::NOT_FOUND);
            body["session"] = json!(fixture.session);
            body["agent"] = json!("unknown");
            assert_eq!(call_action(&fixture, $route, body).await.unwrap_err().0, StatusCode::NOT_FOUND);
            assert_eq!(call_action(&fixture, $route, json!({})).await.unwrap_err().0, StatusCode::BAD_REQUEST);
            assert_eq!(action_calls(&fixture), calls);
        }
    };
}

target_route_test!(
    send_route,
    "/api/chat/send",
    json!({"text": "hello ' $()\nworld"}),
    "type orchestrator"
);
target_route_test!(
    interrupt_route,
    "/api/chat/interrupt",
    json!({}),
    "interrupt orchestrator"
);
target_route_test!(
    key_route,
    "/api/chat/key",
    json!({"key": "Up"}),
    "key orchestrator Up"
);
target_route_test!(
    answer_route,
    "/api/chat/answer",
    json!({"prompt_id": "prompt", "choice": 1}),
    "answer orchestrator prompt 1"
);
target_route_test!(
    close_route,
    "/api/agent/close",
    json!({}),
    "close --now orchestrator"
);

macro_rules! session_route_test {
    ($name:ident, $route:literal, $verb:literal) => {
        #[tokio::test]
        async fn $name() {
            let fixture = action_fixture();
            assert_eq!(call_action(&fixture, $route, json!({"ids": [fixture.session]})).await.unwrap().0, json!({"ok": true}));
            let calls = action_calls(&fixture);
            assert!(calls.contains(&format!("session {} {}", $verb, fixture.session)));
            assert_eq!(call_action(&fixture, $route, json!({"ids": [fixture.session, "unknown"]})).await.unwrap_err().0, StatusCode::NOT_FOUND);
            for bad in [json!({}), json!({"ids": []}), json!({"ids": [1]})] {
                assert_eq!(call_action(&fixture, $route, bad).await.unwrap_err().0, StatusCode::BAD_REQUEST);
            }
            assert_eq!(action_calls(&fixture), calls);
        }
    };
}

session_route_test!(archive_route, "/api/sessions/archive", "archive");
session_route_test!(unarchive_route, "/api/sessions/unarchive", "unarchive");

#[tokio::test]
async fn archived_sessions_route_filters_active_rows() {
    let fixture = action_fixture();
    let before = fixture.app.hub.lock().unwrap().version;
    let value = archived_sessions(State(fixture.app.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(
        value,
        json!({"ok": true, "sessions": [{"id": "archived", "archivedAt": 42}]})
    );
    assert!(action_calls(&fixture).ends_with("|sessions --json --archived\n"));
    assert_eq!(fixture.app.hub.lock().unwrap().version, before);
}

#[tokio::test]
async fn invalid_values_fail_before_a_cli_call_and_send_preserves_stdin() {
    let fixture = action_fixture();
    for (route, fields) in [
        ("/api/chat/send", json!({"text": "  "})),
        ("/api/chat/key", json!({"key": "Escape"})),
        ("/api/chat/answer", json!({"prompt_id": "p", "choice": -1})),
        ("/api/chat/answer", json!({"prompt_id": "", "choice": 0})),
        (
            "/api/chat/interrupt",
            json!({"env": {"SWARM_ADAPTER": "bad"}}),
        ),
    ] {
        let mut body = json!({"session": fixture.session, "agent": "worker"});
        for (key, value) in fields.as_object().unwrap() {
            body[key] = value.clone();
        }
        assert_eq!(
            call_action(&fixture, route, body).await.unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        call_action(
            &fixture,
            "/api/chat/interrupt",
            json!([fixture.session, "chair"])
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(action_calls(&fixture), "");
    let text = "hello ' $()\nworld";
    let _ = call_action(
        &fixture,
        "/api/chat/send",
        json!({"session": fixture.session, "agent": "worker", "text": text}),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap(),
        text
    );
}

#[tokio::test]
async fn action_queue_serializes_calls_and_preserves_cli_errors_without_retry() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
printf 'start\n' >> "$SWARM_HOME/actions"
sleep 0.01
printf 'end\n' >> "$SWARM_HOME/actions"
printf 'exact CLI error\n' >&2
exit 1
"#,
    )
    .unwrap();
    let one = call_action(
        &fixture,
        "/api/chat/interrupt",
        json!({"session": fixture.session, "agent": "worker"}),
    );
    let two = call_action(
        &fixture,
        "/api/chat/interrupt",
        json!({"session": fixture.session, "agent": "worker"}),
    );
    let (one, two) = tokio::join!(one, two);
    for result in [one, two] {
        let (status, Json(error)) = result.unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(error["error"], "exact CLI error");
    }
    assert_eq!(action_calls(&fixture), "start\nend\nstart\nend\n");
}

#[tokio::test]
async fn a_session_send_finishes_while_another_session_action_or_launch_is_running() {
    for slow_route in ["/api/chat/interrupt", "/api/chat/new"] {
        let fixture = action_fixture();
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        let other = swarm::store::create_session(
            &connection,
            "lane",
            Path::new(env!("CARGO_MANIFEST_DIR")),
            None,
            Some("fixture"),
        )
        .unwrap();
        swarm::store::add_agent(&connection, &other, "worker", "chat").unwrap();
        let new_id = uuid::Uuid::now_v7();
        std::fs::write(
            &fixture.app.cli,
            format!(
                r#"#!/bin/sh
case "$1 $2" in
  'session new') printf '%s\n' '{new_id}'; exit 0 ;;
esac
if [ "$1" = interrupt ] || [ "$1" = launch ]; then
  printf 'running\n' > "$SWARM_HOME/slow-start"
  while [ ! -f "$SWARM_HOME/release" ]; do sleep 0.01; done
  printf 'fixture-pane\n'
fi
if [ "$1" = type ]; then cat > "$SWARM_HOME/text"; fi
"#
            ),
        )
        .unwrap();
        let slow_app = fixture.app.clone();
        let slow_body = if slow_route == "/api/chat/new" {
            json!({"cwd": env!("CARGO_MANIFEST_DIR")})
        } else {
            json!({"session": other, "agent": "worker"})
        };
        let slow = tokio::spawn(async move {
            chat_action(
                State(slow_app),
                slow_route.parse().unwrap(),
                Ok(Json(slow_body)),
            )
            .await
        });
        let home = fixture.app.root.parent().unwrap();
        tokio::time::timeout(Duration::from_secs(30), async {
            while !home.join("slow-start").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let send = tokio::time::timeout(
            Duration::from_secs(30),
            call_action(
                &fixture,
                "/api/chat/send",
                json!({"session": fixture.session, "agent": "worker", "text": "parallel send"}),
            ),
        )
        .await;
        let still_running = !slow.is_finished();
        // Release the stand-in before an assertion so a failed test cannot leave it waiting.
        std::fs::write(home.join("release"), "release").unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(30), slow)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(still_running, "slow action ended before the send finished");
        assert_eq!(
            send.expect("send must not wait for another session or launch")
                .unwrap()
                .0,
            json!({"ok": true})
        );
        assert_eq!(
            std::fs::read_to_string(home.join("text")).unwrap(),
            "parallel send"
        );
    }
}

#[tokio::test]
async fn archive_and_unarchive_share_a_lane_without_blocking_a_session_send() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
case "$1 $2" in
  'session archive')
    printf 'running\n' > "$SWARM_HOME/slow-start"
    while [ ! -f "$SWARM_HOME/release" ]; do sleep 0.01; done ;;
  'session unarchive') printf 'restored\n' > "$SWARM_HOME/restored" ;;
  *) if [ "$1" = type ]; then cat > "$SWARM_HOME/text"; fi ;;
esac
"#,
    )
    .unwrap();
    let home = fixture.app.root.parent().unwrap();
    let archive_app = fixture.app.clone();
    let ids = json!({"ids": [fixture.session]});
    let archive_ids = ids.clone();
    let archive = tokio::spawn(async move {
        chat_action(
            State(archive_app),
            "/api/sessions/archive".parse().unwrap(),
            Ok(Json(archive_ids)),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(30), async {
        while !home.join("slow-start").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let restore_app = fixture.app.clone();
    let restore = tokio::spawn(async move {
        chat_action(
            State(restore_app),
            "/api/sessions/unarchive".parse().unwrap(),
            Ok(Json(ids)),
        )
        .await
    });
    let send = tokio::time::timeout(
        Duration::from_secs(30),
        call_action(
            &fixture,
            "/api/chat/send",
            json!({"session": fixture.session, "agent": "worker", "text": "parallel send"}),
        ),
    )
    .await;
    let restore_waited = !restore.is_finished() && !home.join("restored").exists();
    std::fs::write(home.join("release"), "release").unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(30), archive)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(30), restore)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        restore_waited,
        "unarchive must wait for archive in the shared lane"
    );
    assert_eq!(
        send.expect("send must not wait for the shared lane")
            .unwrap()
            .0,
        json!({"ok": true})
    );
    assert!(home.join("restored").exists());
}
