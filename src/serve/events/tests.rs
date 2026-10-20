//! Tests for server events.

use super::*;
use crate::serve::test_support::*;

#[test]
fn queue_overflow_resets_every_affected_key_and_stays_bounded() {
    let mut stream = client(&[("s:one", 0), ("s:two", 0)]);
    for index in 0..QUEUE_LIMIT {
        let key = if index % 2 == 0 { "s:one" } else { "s:two" };
        stream.push(Frame::new("transcript", json!({"index": index}), Some(key)));
    }
    stream.push(Frame::new("transcript", json!({}), Some("s:three")));
    assert!(stream.queue.len() <= QUEUE_LIMIT);
    assert_eq!(stream.queue.len(), 3);
    for key in ["s:one", "s:two", "s:three"] {
        assert!(
            stream
                .queue
                .iter()
                .any(|frame| frame.name == "reset" && frame.key.as_deref() == Some(key))
        );
    }
    for version in 0..2000 {
        stream.push(Frame::new("snapshot", json!({"version": version}), None));
    }
    assert_eq!(stream.queue.len(), 4);
    assert_eq!(
        serde_json::from_str::<Value>(&stream.queue.back().unwrap().data).unwrap()["version"],
        1999
    );
}
#[test]
fn cold_row_cache_keeps_only_four_keys() {
    let fixture = Fixture::new();
    let mut hub = fixture.app.hub.lock().unwrap();
    for number in 0..12 {
        hub.cache_rows(format!("chat-{number}"), LiveRows::empty(0, false));
    }
    assert_eq!(hub.row_cache.len(), 4);
    assert_eq!(hub.row_cache[0].0, "chat-8");
}
#[tokio::test]
async fn twelve_row_keys_keep_live_state_and_drop_all_children() {
    let fixture = Fixture::new();
    codex_log(&fixture, 2);
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    for number in 0..12 {
        let agent = format!("worker-{number}");
        swarm::store::add_agent(&connection, &fixture.session, &agent, "chat").unwrap();
        swarm::store::set_provider(&connection, &fixture.session, &agent, "codex").unwrap();
        swarm::store::set_log(&connection, &fixture.session, &agent, &fixture.log).unwrap();
    }
    drop(connection);
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .insert("all".into(), client(&[]));
    let from = std::fs::metadata(&fixture.log).unwrap().len();
    let keys = (0..12)
        .map(|number| SubscriptionKey {
            key: format!("{}:worker-{number}", fixture.session),
            from,
            view: Some("rows".into()),
        })
        .collect();
    let _ = subscribe(
        State(fixture.app.clone()),
        Json(Subscription {
            stream: "all".into(),
            keys,
        }),
    )
    .await
    .unwrap();
    let mut receivers = {
        let hub = fixture.app.hub.lock().unwrap();
        assert_eq!(hub.followers.len(), 12);
        assert!(hub.followers.values().all(|follower| {
            follower
                .rows
                .as_ref()
                .is_some_and(|rows| rows.records.len() == 2)
        }));
        hub.followers
            .values()
            .map(|follower| follower.done.clone())
            .collect::<Vec<_>>()
    };
    drop(Events {
        app: fixture.app.clone(),
        id: "all".into(),
    });
    for done in &mut receivers {
        tokio::time::timeout(Duration::from_secs(2), done.wait_for(|finished| *finished))
            .await
            .unwrap()
            .unwrap();
    }
    let hub = fixture.app.hub.lock().unwrap();
    assert!(hub.followers.is_empty());
    assert_eq!(hub.row_cache.len(), 4);
}
