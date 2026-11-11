use super::*;
use std::io::Write;

const TEXT: &str = "The distant lighthouse guides the boats home";

fn queued(app: &App) -> Vec<(String, Value)> {
    let hub = app.hub.lock().unwrap();
    hub.streams["all"]
        .queue
        .iter()
        .map(|frame| {
            let mut actual: Value = serde_json::from_str(&frame.data).unwrap();
            if frame.name == "live" {
                actual.as_object_mut().unwrap().remove("live_body");
            }
            (frame.name.to_owned(), actual)
        })
        .collect()
}
fn expected(key: &str, text: Option<&str>, ended: bool, hide_text: bool) -> Value {
    let display = text.map(|text| json!({"blocks": [{"type": "paragraph", "value": [{"type": "text", "value": text}]}]}));
    json!({"key": key, "text": text, "thinking": null, "display": display,
        "ended": ended, "hide_text": hide_text, "hide_thinking": false})
}
async fn wait_for(app: &App, condition: impl Fn(&[(String, Value)]) -> bool) {
    tokio::time::timeout(Duration::from_millis(900), async {
        loop {
            if condition(&queued(app)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("expected live state did not reach the client queue");
}
async fn pi_turn() -> (Fixture, PathBuf, String) {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    swarm::store::add_agent(
        &connection,
        &fixture.session,
        "orchestrator",
        "orchestrator",
    )
    .unwrap();
    swarm::store::set_provider(&connection, &fixture.session, "orchestrator", "pi").unwrap();
    drop(connection);
    let file = swarm::bus::pi_live_file(&fixture.app.root, &fixture.session, "orchestrator");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, json!({"text": TEXT}).to_string()).unwrap();
    {
        let mut hub = fixture.app.hub.lock().unwrap();
        hub.agents = json!({fixture.session.as_str(): [{"id": "orchestrator", "pane": "%1", "alive": true, "state": "working"}]});
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
    wait_for(&fixture.app, |frames| {
        frames.iter().any(|(name, _)| name == "live")
    })
    .await;
    let frames: Vec<_> = queued(&fixture.app)
        .into_iter()
        .filter(|(name, _)| name == "live")
        .collect();
    assert_eq!(
        frames,
        vec![("live".into(), expected(&key, Some(TEXT), false, false))]
    );
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .get_mut("all")
        .unwrap()
        .queue
        .clear();
    (fixture, file, key)
}
fn append(fixture: &Fixture, role: &str, text: &str) {
    let record = json!({"type": role, "uuid": uuid::Uuid::now_v7().to_string(),
        "message": {"role": role, "content": [{"type": "text", "text": text}]}});
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(&fixture.log)
            .unwrap(),
        "{record}"
    )
    .unwrap();
}

#[tokio::test]
async fn landed_text_hides_after_its_rows_frame() {
    let (fixture, _, key) = pi_turn().await;
    append(&fixture, "assistant", TEXT);
    wait_for(&fixture.app, |frames| {
        frames
            .iter()
            .any(|(name, frame)| name == "live" && frame["hide_text"] == true)
    })
    .await;
    let frames = queued(&fixture.app);
    let rows = frames
        .iter()
        .position(|(name, frame)| {
            name == "rows"
                && frame["upsert"].as_array().is_some_and(|rows| {
                    rows.iter()
                        .any(|row| row["kind"] == "assistant" && row["text"] == TEXT)
                })
        })
        .unwrap();
    let hide = frames
        .iter()
        .position(|(name, frame)| name == "live" && frame["hide_text"] == true)
        .unwrap();
    assert!(
        rows < hide,
        "the client must receive the landed row before its hide frame"
    );
    let live: Vec<_> = frames
        .into_iter()
        .filter(|(name, _)| name == "live")
        .collect();
    assert_eq!(
        live,
        vec![("live".into(), expected(&key, Some(TEXT), false, true))]
    );
    drop(Events {
        app: fixture.app.clone(),
        id: "all".into(),
    });
}

#[tokio::test]
async fn ended_text_stays_until_an_unrelated_row_lands() {
    let (fixture, file, key) = pi_turn().await;
    std::fs::write(file, "").unwrap();
    wait_for(&fixture.app, |frames| {
        frames
            .iter()
            .any(|(name, frame)| name == "live" && frame["ended"] == true)
    })
    .await;
    assert_eq!(
        queued(&fixture.app),
        vec![("live".into(), expected(&key, Some(TEXT), true, false))]
    );
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .get_mut("all")
        .unwrap()
        .queue
        .clear();
    append(&fixture, "user", "Please check the weather");
    wait_for(&fixture.app, |frames| {
        frames
            .iter()
            .any(|(name, frame)| name == "live" && frame["text"].is_null())
    })
    .await;
    let frames = queued(&fixture.app);
    let rows = frames.iter().position(|(name, _)| name == "rows").unwrap();
    let clear = frames.iter().position(|(name, _)| name == "live").unwrap();
    assert!(
        rows < clear,
        "the client must receive the row before the ended state clears"
    );
    let live: Vec<_> = frames
        .into_iter()
        .filter(|(name, _)| name == "live")
        .collect();
    assert_eq!(
        live,
        vec![("live".into(), expected(&key, None, false, false))]
    );
    drop(Events {
        app: fixture.app.clone(),
        id: "all".into(),
    });
}
