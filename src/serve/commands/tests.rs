//! Tests for server commands.

use super::*;
use crate::serve::test_support::*;

#[test]
fn a_menu_read_with_a_note_keeps_the_last_fuller_rows() {
    let full = json!({"commands": [{"name": "compact"}, {"name": "skill:plan"}], "note": null});
    let partial = json!({"commands": [{"name": "compact"}], "note": "pi list unavailable"});
    let kept = keep_fuller_rows(Some(&full), partial.clone());
    assert_eq!(kept["commands"], full["commands"]);
    assert_eq!(kept["note"], "pi list unavailable");
    let now = tokio::time::Instant::now();
    assert_eq!(menu_entry(Ok(partial.clone()), now).0, now + COMMANDS_RETRY);
    assert_eq!(menu_entry(Ok(full.clone()), now).0, now + COMMANDS_FRESH);
    let first = keep_fuller_rows(None, partial.clone());
    assert_eq!(first, partial, "with no earlier list the partial rows stay");
    let fresh = json!({"commands": [{"name": "compact"}], "note": null});
    assert_eq!(
        keep_fuller_rows(Some(&full), fresh.clone()),
        fresh,
        "a full read replaces the rows"
    );
}

#[tokio::test]
async fn commands_share_one_probe_serve_the_cache_and_refresh_it_behind_the_reply() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
printf '%s\n' "$SWARM_SESSION_ID|$*" >> "$SWARM_HOME/actions"
if [ -f "$SWARM_HOME/fail" ]; then printf 'probe failed' >&2; exit 2; fi
sleep 0.2
printf '{"commands":[{"insert":"/clear"}],"note":null}\n'
"#,
    )
    .unwrap();
    let read = |agent: &str| {
        chat_commands(
            State(fixture.app.clone()),
            Query(CommandsQuery {
                session: fixture.session.clone(),
                agent: agent.into(),
            }),
        )
    };
    let probes = || {
        action_calls(&fixture)
            .matches("commands orchestrator --json")
            .count()
    };
    let (one, two) = tokio::join!(read("chair"), read("orchestrator"));
    assert_eq!(one.unwrap().0["commands"][0]["insert"], "/clear");
    assert_eq!(two.unwrap().0["commands"][0]["insert"], "/clear");
    assert_eq!(probes(), 1, "two reads at once share one probe");
    assert!(action_calls(&fixture).starts_with(&format!("{}|", fixture.session)));

    let _ = read("chair").await.unwrap();
    assert_eq!(probes(), 1, "a fresh list is served from the cache");

    std::fs::write(fixture.app.root.parent().unwrap().join("fail"), "").unwrap();
    let slot = fixture
        .app
        .commands
        .lock()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    slot.last.lock().unwrap().as_mut().unwrap().0 = tokio::time::Instant::now();
    let stale = read("chair").await.unwrap().0;
    assert_eq!(
        stale["commands"][0]["insert"], "/clear",
        "a stale list returns at once"
    );
    for _ in 0..50 {
        if probes() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(probes(), 2, "the stale read refreshed behind the reply");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let failed = read("chair").await.unwrap().0;
    assert_eq!(
        failed["commands"][0]["insert"], "/clear",
        "a failed probe keeps the earlier rows"
    );
    assert_eq!(failed["note"], "probe failed");
    let _ = read("chair").await.unwrap();
    assert_eq!(probes(), 2, "a failure is not probed again at once");

    let unknown = chat_commands(
        State(fixture.app.clone()),
        Query(CommandsQuery {
            session: fixture.session.clone(),
            agent: "nobody".into(),
        }),
    )
    .await;
    assert_eq!(unknown.unwrap_err().0, StatusCode::NOT_FOUND);
}
