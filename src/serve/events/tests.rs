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
#[tokio::test]
async fn show_tells_each_open_page_to_open_the_chat() {
    let fixture = Fixture::new();
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .insert("all".into(), client(&[]));
    let Json(answer) = show(
        State(fixture.app.clone()),
        Ok(Json(Show {
            session: "01a1-b2".into(),
        })),
    )
    .await
    .unwrap();
    assert_eq!(answer["pages"], 1);
    {
        let hub = fixture.app.hub.lock().unwrap();
        let frame = hub.streams["all"].queue.back().unwrap();
        assert_eq!(frame.name, "show");
        assert_eq!(
            serde_json::from_str::<Value>(&frame.data).unwrap()["session"],
            "01a1-b2"
        );
    }
    let bad = show(
        State(fixture.app.clone()),
        Ok(Json(Show {
            session: "../x".into(),
        })),
    )
    .await;
    assert!(bad.is_err());
}
#[tokio::test]
async fn a_working_chair_streams_its_screen_text_and_the_turn_end_clears_it() {
    let fixture = Fixture::new();
    let screen = fixture.dir.join("screen.txt");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens/claude-turn-text.txt"),
        &screen,
    )
    .unwrap();
    std::fs::create_dir_all(fixture.app.root.join("adapters")).unwrap();
    std::fs::write(
        fixture.app.root.join("adapters/tmux-swarm.conf"),
        format!("capture = cat '{}'\n", screen.display()),
    )
    .unwrap();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    swarm::store::add_agent(
        &connection,
        &fixture.session,
        "orchestrator",
        "orchestrator",
    )
    .unwrap();
    drop(connection);
    let agents = |state: &str| {
        let agent = json!({"id": "orchestrator", "pane": "%1", "alive": true, "state": state});
        json!({ fixture.session.as_str(): [agent] })
    };
    {
        let mut hub = fixture.app.hub.lock().unwrap();
        hub.agents = agents("working");
        hub.streams.insert("all".into(), client(&[]));
    }
    // The page names the chair's chat `chair`; the store names it `orchestrator`.
    let key = format!("{}:chair", fixture.session);
    let from = std::fs::metadata(&fixture.log).unwrap().len();
    let _ = subscribe(
        State(fixture.app.clone()),
        Json(Subscription {
            stream: "all".into(),
            keys: vec![SubscriptionKey {
                key: key.clone(),
                from,
                view: Some("rows".into()),
            }],
        }),
    )
    .await
    .unwrap();
    let live = |app: &App| {
        let hub = app.hub.lock().unwrap();
        hub.streams["all"]
            .queue
            .iter()
            .filter(|frame| frame.name == "live")
            .map(|frame| serde_json::from_str::<Value>(&frame.data).unwrap())
            .collect::<Vec<_>>()
    };
    until_live(&fixture.app, &live, 1).await;
    let frames = live(&fixture.app);
    assert_eq!(frames[0]["key"], key.as_str());
    assert!(
        frames[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("In the 1860s, French makers")
    );
    // A failed read keeps the shown text, so the row does not blink.
    std::fs::remove_file(&screen).unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(live(&fixture.app).len(), 1);
    fixture.app.hub.lock().unwrap().agents = agents("idle");
    until_live(&fixture.app, &live, 2).await;
    let frames = live(&fixture.app);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[1]["text"], Value::Null);
    drop(Events {
        app: fixture.app.clone(),
        id: "all".into(),
    });
}

async fn until_live(app: &App, live: &impl Fn(&App) -> Vec<Value>, count: usize) {
    for _ in 0..40 {
        if live(app).len() >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_new_watcher_tells_the_page_that_no_turn_runs() {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    swarm::store::add_agent(
        &connection,
        &fixture.session,
        "orchestrator",
        "orchestrator",
    )
    .unwrap();
    drop(connection);
    {
        let mut hub = fixture.app.hub.lock().unwrap();
        let agent = json!({"id": "orchestrator", "pane": "%1", "alive": true, "state": "idle"});
        hub.agents = json!({ fixture.session.as_str(): [agent] });
        hub.streams.insert("all".into(), client(&[]));
    }
    let key = format!("{}:chair", fixture.session);
    let from = std::fs::metadata(&fixture.log).unwrap().len();
    let _ = subscribe(
        State(fixture.app.clone()),
        Json(Subscription {
            stream: "all".into(),
            keys: vec![SubscriptionKey {
                key: key.clone(),
                from,
                view: Some("rows".into()),
            }],
        }),
    )
    .await
    .unwrap();
    // A page that kept an old turn's text from an earlier watcher gets null at once.
    let live = |app: &App| {
        let hub = app.hub.lock().unwrap();
        hub.streams["all"]
            .queue
            .iter()
            .filter(|frame| frame.name == "live")
            .map(|frame| serde_json::from_str::<Value>(&frame.data).unwrap())
            .collect::<Vec<_>>()
    };
    until_live(&fixture.app, &live, 1).await;
    let frames = live(&fixture.app);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["text"], Value::Null);
    drop(Events {
        app: fixture.app.clone(),
        id: "all".into(),
    });
}
