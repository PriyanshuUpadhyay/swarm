//! Tests for server follow.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn shared_followers_replay_late_subscribers_and_stop_after_the_last_reference() {
    let fixture = Fixture::new();
    let key = format!("{}:chair", fixture.session);
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .insert("one".into(), client(&[]));
    let _ = subscribe(
        State(fixture.app.clone()),
        Json(Subscription {
            stream: "one".into(),
            keys: vec![SubscriptionKey {
                key: key.clone(),
                from: 0,
                view: None,
            }],
        }),
    )
    .await
    .unwrap();
    let size = std::fs::metadata(&fixture.log).unwrap().len();
    until_cursor(&fixture.app, "one", &key, size).await;
    let generation = {
        let mut hub = fixture.app.hub.lock().unwrap();
        hub.streams.get_mut("one").unwrap().queue.clear();
        hub.streams.insert("two".into(), client(&[]));
        hub.followers[&key].generation.clone()
    };
    let _ = subscribe(
        State(fixture.app.clone()),
        Json(Subscription {
            stream: "two".into(),
            keys: vec![SubscriptionKey {
                key: key.clone(),
                from: 0,
                view: None,
            }],
        }),
    )
    .await
    .unwrap();
    until_cursor(&fixture.app, "two", &key, size).await;
    {
        let mut hub = fixture.app.hub.lock().unwrap();
        assert_eq!(hub.followers.len(), 1);
        assert_ne!(hub.followers[&key].generation, generation);
        assert!(hub.streams["one"].queue.is_empty());
        assert!(
            hub.streams["two"]
                .queue
                .iter()
                .any(|frame| frame.name == "transcript")
        );
        hub.streams.get_mut("two").unwrap().queue.clear();
    }
    let line = "{\"type\":\"user\",\"uuid\":\"synthetic-live\",\"message\":{\"content\":\"live boundary\"}}\n";
    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(&fixture.log)
        .unwrap();
    log.write_all(&line.as_bytes()[..line.len() - 1]).unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        fixture.app.hub.lock().unwrap().streams["one"]
            .queue
            .is_empty()
    );
    let started = std::time::Instant::now();
    log.write_all(b"\n").unwrap();
    until_cursor(&fixture.app, "one", &key, size + line.len() as u64).await;
    until_cursor(&fixture.app, "two", &key, size + line.len() as u64).await;
    assert!(started.elapsed() < Duration::from_secs(1));
    {
        let hub = fixture.app.hub.lock().unwrap();
        let first =
            serde_json::from_str::<Value>(&hub.streams["one"].queue.back().unwrap().data).unwrap();
        let second =
            serde_json::from_str::<Value>(&hub.streams["two"].queue.back().unwrap().data).unwrap();
        assert_eq!(first, second);
        assert_eq!(first["events"][0]["text"], "live boundary");
        assert_eq!(first["end_offset"], size + line.len() as u64);
    }
    let mut done = fixture.app.hub.lock().unwrap().followers[&key].done.clone();
    drop(Events {
        app: fixture.app.clone(),
        id: "one".into(),
    });
    assert_eq!(fixture.app.hub.lock().unwrap().followers.len(), 1);
    drop(Events {
        app: fixture.app.clone(),
        id: "two".into(),
    });
    tokio::time::timeout(Duration::from_secs(1), done.wait_for(|value| *value))
        .await
        .unwrap()
        .unwrap();
    assert!(fixture.app.hub.lock().unwrap().followers.is_empty());
}
#[tokio::test]
async fn rows_and_raw_clients_share_one_follower_and_stable_tool_id() {
    let fixture = Fixture::new();
    let call = json!({"type":"assistant","uuid":"call","message":{"content":[{"type":"tool_use","id":"live-tool","name":"Bash","input":{"command":"print"}}]}}).to_string()+"\n";
    std::fs::write(&fixture.log, call).unwrap();
    let page = row_page(&fixture, 200, None).await.unwrap();
    let call_id = page["rows"][0]["id"].clone();
    let key = format!("{}:chair", fixture.session);
    let from = page["page"]["end_offset"].as_u64().unwrap();
    {
        let mut hub = fixture.app.hub.lock().unwrap();
        hub.streams.insert("rows".into(), client(&[]));
        hub.streams.insert("raw".into(), client(&[]));
    }
    for (stream, view) in [("rows", Some("rows")), ("raw", None)] {
        let _ = subscribe(
            State(fixture.app.clone()),
            Json(Subscription {
                stream: stream.into(),
                keys: vec![SubscriptionKey {
                    key: key.clone(),
                    from,
                    view: view.map(str::to_owned),
                }],
            }),
        )
        .await
        .unwrap();
    }
    assert_eq!(fixture.app.hub.lock().unwrap().followers.len(), 1);
    let result = json!({"type":"user","uuid":"result","message":{"content":[{"type":"tool_result","tool_use_id":"live-tool","content":"finished output"}]}}).to_string()+"\n";
    std::fs::OpenOptions::new()
        .append(true)
        .open(&fixture.log)
        .unwrap()
        .write_all(result.as_bytes())
        .unwrap();
    until_cursor(&fixture.app, "rows", &key, from + result.len() as u64).await;
    until_cursor(&fixture.app, "raw", &key, from + result.len() as u64).await;
    {
        let hub = fixture.app.hub.lock().unwrap();
        let frame = hub.streams["rows"].queue.back().unwrap();
        assert_eq!(frame.name, "rows");
        let rows: Value = serde_json::from_str(&frame.data).unwrap();
        assert_eq!(rows["upsert"][0]["id"], call_id);
        assert_eq!(rows["upsert"][0]["tool"]["state"], "finished");
        assert_eq!(hub.streams["raw"].queue.back().unwrap().name, "transcript");
    }
    let mut done = fixture.app.hub.lock().unwrap().followers[&key].done.clone();
    drop(Events {
        app: fixture.app.clone(),
        id: "rows".into(),
    });
    {
        let hub = fixture.app.hub.lock().unwrap();
        assert!(hub.followers[&key].rows.is_none());
        assert_eq!(hub.row_cache.len(), 1);
    }
    // A page read while only raw listeners remain belongs to the bounded inactive cache.
    row_page(&fixture, 200, None).await.unwrap();
    assert!(
        fixture.app.hub.lock().unwrap().followers[&key]
            .rows
            .is_none()
    );
    drop(Events {
        app: fixture.app.clone(),
        id: "raw".into(),
    });
    done.wait_for(|finished| *finished).await.unwrap();
    let hub = fixture.app.hub.lock().unwrap();
    assert!(hub.followers.is_empty());
    assert_eq!(hub.row_cache.len(), 1);
}
