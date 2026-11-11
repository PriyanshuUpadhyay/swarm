include!("legacy_body.rs");
use super::*;

// S8C-QUEUE-V2 T1. A real coordinator reads a private pi screen. The
// readiness deadline is test configuration; assertions do not measure time.
const QUEUE_READY_TIMEOUT: Duration = Duration::from_secs(15);

fn snapshot(server: &Server) -> Value {
    let (status, _, body) = server.request("GET", "/api/snapshot", "", "", true);
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

fn queue(server: &Server, snapshot: &Value) -> Value {
    snapshot["agents"][&server.session]
        .as_array()
        .and_then(|agents| agents.iter().find(|agent| agent["id"] == "orchestrator"))
        .map(|agent| agent["queue"].clone())
        .unwrap_or(Value::Null)
}

fn ready_queue(server: &Server, raw: &Value) -> Value {
    let deadline = Instant::now() + QUEUE_READY_TIMEOUT;
    loop {
        let page = snapshot(server);
        let actual = queue(server, &page);
        let observed = actual.as_array().map(|items| {
            items
                .iter()
                .map(|item| json!({"mode":item["mode"],"text":item["text"]}))
                .collect::<Vec<_>>()
        });
        // Wait for source observations only. New fields must reach the final
        // assert, so their absence on the base cannot become a setup timeout.
        if observed.as_ref().is_some_and(|items| json!(items) == *raw) {
            return page;
        }
        assert!(
            Instant::now() < deadline,
            "queue source not observed: {actual}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn s8c_snapshot_queue_has_served_labels_and_clears_after_a_version_notice() {
    let server = Server::with_actions(true);
    let home = server.dir.join("home");
    // Only these two rule lines are needed by the pi box reader. Keep them
    // after clear: a missing box means an unknown queue, not an empty queue.
    let empty_box = "\u{2500}\n\n\u{2500}\n";
    let two = format!(
        "Steering: first\nFollow-up: second\n\u{21b3} Option+Up to edit all queued messages\n{empty_box}"
    );
    fs::write(home.join("screen"), two).unwrap();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &server.session, "orchestrator", "pi").unwrap();
    drop(connection);
    let initial = ready_queue(
        &server,
        &json!([
            {"mode":"steering","text":"first"},
            {"mode":"follow-up","text":"second"}
        ]),
    );
    let mut live = server.sse();
    named_event(&mut live, "hello");
    fs::write(home.join("screen"), empty_box).unwrap();
    let cleared = ready_queue(&server, &json!([]));
    let notice = named_event(&mut live, "snapshot");
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/s8c/queue.json")).unwrap();
    let actual = without_body(json!({
        "initial":queue(&server, &initial),
        "cleared":queue(&server, &cleared),
        "refresh":notice["version"].as_u64().unwrap() > initial["version"].as_u64().unwrap()
    }));
    assert_eq!(
        actual,
        json!({"initial":fixture["wire"],"cleared":[],"refresh":true})
    );
}
