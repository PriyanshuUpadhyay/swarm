use super::*;

const QUEUE_READY_TIMEOUT: Duration = Duration::from_secs(15);

fn page(server: &Server, path: &str) -> Value {
    let (status, _, body) = server.request("GET", path, "", "", true);
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

fn snapshot_queue(server: &Server, raw: &Value) -> Value {
    let deadline = Instant::now() + QUEUE_READY_TIMEOUT;
    loop {
        let snapshot = page(server, "/api/snapshot");
        let queue = snapshot["agents"][&server.session]
            .as_array()
            .and_then(|agents| agents.iter().find(|agent| agent["id"] == "orchestrator"))
            .map(|agent| agent["queue"].clone())
            .unwrap_or(Value::Null);
        let text: Vec<_> = queue
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| item["text"].clone())
            .collect();
        if json!(text) == *raw {
            return queue;
        }
        assert!(
            Instant::now() < deadline,
            "queue source not observed: {queue}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

// S8D-IMAGES-V2 T1. HAND paths and source ids come from the input bytes,
// never from candidate output. Both page sites and the nonzero live cursor run.
#[test]
fn s8d_images_cover_pages_cached_tail_live_rows_and_snapshot_queue() {
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/s8d/images.json")).unwrap();
    let server = Server::with_actions(true);
    fs::write(&server.log, fixture["log"].as_str().unwrap()).unwrap();
    let path = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    let first = page(&server, &path);
    let cached = page(&server, &path);
    let mut live = server.sse();
    let stream = named_event(&mut live, "hello")["stream"].clone();
    let subscription = json!({"stream":stream,"keys":[{
        "key":format!("{}:chair",server.session),"from":fixture["cursor"],"view":"rows"
    }]});
    assert_eq!(
        server
            .request(
                "POST",
                "/api/subscribe",
                "Content-Type: application/json\r\n",
                &subscription.to_string(),
                true
            )
            .0,
        200
    );
    server.append(fixture["append"].as_str().unwrap());
    let frame = named_event(&mut live, "rows");

    let home = server.dir.join("home");
    fs::write(home.join("screen"), format!(
        "Steering: {}\nFollow-up: {}\n\u{21b3} Option+Up to edit all queued messages\n\u{2500}\n\n\u{2500}\n",
        fixture["two"].as_str().unwrap(), fixture["other"].as_str().unwrap()
    )).unwrap();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &server.session, "orchestrator", "pi").unwrap();
    drop(connection);
    let queue = snapshot_queue(&server, &json!([fixture["two"], fixture["other"]]));
    let wire_item = |index: usize, mode: &str, detail: &str, text: &Value, images: Value| {
        json!({
            "id":format!("queue-{index}"),"mode":mode,"detail":detail,"text":text,
            "display":{"blocks":[{"type":"paragraph","value":[{"type":"text","value":text}]}]},
            "images":images
        })
    };
    assert_eq!(
        json!({"page":first["images"],"cached":cached["images"],"live":frame["images"],"queue":queue}),
        json!({"page":fixture["page_images"],"cached":fixture["page_images"],"live":fixture["live_images"],"queue":[
            wire_item(0,"steering","Steering",&fixture["two"],json!([fixture["a"],fixture["b"]])),
            wire_item(1,"follow-up","Follow-up",&fixture["other"],json!([]))
        ]})
    );
}
