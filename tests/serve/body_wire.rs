use super::*;

fn page(server: &Server, path: &str) -> Value {
    let (status, _, body) = server.request("GET", path, "", "", true);
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

fn record(id: &str, role: &str, block: &str, text: &str) -> String {
    let content = if block == "thinking" {
        json!({"type":"thinking","thinking":text})
    } else {
        json!({"type":"text","text":text})
    };
    json!({"type":role,"uuid":id,"message":{"content":[content]}}).to_string() + "\n"
}

fn flattened(rows: &Value) -> Vec<&Value> {
    let mut found = Vec::new();
    let mut pending = rows.as_array().unwrap().iter().collect::<Vec<_>>();
    while let Some(row) = pending.pop() {
        found.push(row);
        if let Some(steps) = row["steps"].as_array() {
            pending.extend(steps);
        }
    }
    found
}

// S10e-2 A. Modes are frozen ORIGINAL/HAND choices, never Rust output.
// The HTTP inputs use real Markdown; exact synthetic Later trees are part B.
#[test]
fn s10e2_body_eligibility_through_get_rows() {
    let cases: Value = serde_json::from_str(include_str!("../fixtures/s10e2/http.json")).unwrap();
    let server = Server::new();
    let mut log = (0..32)
        .map(|i| record(&format!("pad-{i}"), "user", "text", "pad"))
        .collect::<String>();
    for case in cases.as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        log.push_str(&record(&format!("separator-{id}"), "user", "text", id));
        log.push_str(&record(
            id,
            "assistant",
            case["block"].as_str().unwrap_or("text"),
            case["text"].as_str().unwrap(),
        ));
    }
    fs::write(&server.log, log).unwrap();
    let path = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    let response = page(&server, &path);
    let rows = flattened(&response["rows"]);
    let mut observed = serde_json::Map::new();
    let mut expected = serde_json::Map::new();
    for case in cases.as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let row = rows
            .iter()
            .find(|row| row["text"] == case["text"] && row["kind"] != "user")
            .unwrap_or_else(|| panic!("source row missing for {id}: {}", response["rows"]));
        let row_id = row["id"].as_str().unwrap();
        // Prove the source reaches the intended Display presence before testing body.
        assert_eq!(
            response["display"][row_id].is_object(),
            case["body"] == "tree",
            "{id}: Display reachability"
        );
        observed.insert(row_id.to_owned(), response["body"][row_id].clone());
        expected.insert(row_id.to_owned(), case["body"].clone());
    }
    assert_eq!(
        Value::Object(observed),
        Value::Object(expected),
        "body map absent or choices differ"
    );
}

fn pair(response: &Value, rows_key: &str) -> Value {
    let rows = flattened(&response[rows_key]);
    let mut modes = serde_json::Map::new();
    for row in rows {
        let id = row["id"].as_str().unwrap();
        if id.starts_with("pair-pad-") {
            continue;
        }
        // Frozen no-Later controls: prose/empty Display is tree; fold/tool has no Display.
        let expected = if row["kind"] == "fold" || row["tool"].is_object() {
            "plain"
        } else {
            "tree"
        };
        modes.insert(
            id.to_owned(),
            json!({"actual":response["body"][id],"expected":expected}),
        );
    }
    json!(modes)
}

fn ready_queue(server: &Server, texts: &[&str]) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let snapshot = page(server, "/api/snapshot");
        let queue = snapshot["agents"][&server.session]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == "orchestrator")
            .unwrap()["queue"]
            .clone();
        let observed = queue.as_array().map(|items| {
            items
                .iter()
                .map(|q| q["text"].as_str().unwrap())
                .collect::<Vec<_>>()
        });
        if observed.as_deref() == Some(texts) {
            return queue;
        }
        assert!(
            Instant::now() < deadline,
            "queue source not observed: {queue}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn queue_modes(queue: &Value) -> Value {
    json!(
        queue
            .as_array()
            .unwrap()
            .iter()
            .map(|q| json!({"id":q["id"],"text":q["text"],"body":q["body"]}))
            .collect::<Vec<_>>()
    )
}

// S10e-2 C. All inputs here have no Later. This checks transport pairing only.
#[test]
fn s10e2_body_pairs_merged_cached_steps_sse_and_queue_refresh() {
    let server = Server::with_actions(true);
    // Two tools and a collapsed multiline compaction produce a Markdown step.
    let mut seed = (0..32)
        .map(|i| record(&format!("pair-pad-{i}"), "user", "text", "pad"))
        .collect::<String>();
    seed.push_str(&(json!({"type":"assistant","uuid":"pair-call","message":{"content":[{"type":"tool_use","id":"pair-tool","name":"Bash","input":{"command":"print"}}]}}).to_string()+"\n"));
    seed.push_str(&(json!({"type":"user","uuid":"pair-result","message":{"content":[{"type":"tool_result","tool_use_id":"pair-tool","content":"done"}]}}).to_string()+"\n"));
    seed.push_str(
        &(json!({"type":"system","uuid":"pair-step","subtype":"compact_boundary","content":"\n"})
            .to_string()
            + "\n"),
    );
    seed.push_str(&(json!({"type":"assistant","uuid":"pair-call-two","message":{"content":[{"type":"tool_use","id":"pair-tool-two","name":"Bash","input":{"command":"print"}}]}}).to_string()+"\n"));
    seed.push_str(&(json!({"type":"user","uuid":"pair-result-two","message":{"content":[{"type":"tool_result","tool_use_id":"pair-tool-two","content":"done"}]}}).to_string()+"\n"));
    seed.push_str(&record("pair-answer", "assistant", "text", "kept"));
    fs::write(&server.log, &seed).unwrap();
    let path = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    let merged = page(&server, &path);
    assert!(
        merged["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["steps"].as_array().is_some_and(|steps| steps
                .iter()
                .any(|s| s["text"] == "\n" && s["system_kind"] == "compaction"))),
        "source must have a Markdown step"
    );
    let mut live = server.sse();
    let stream = named_event(&mut live, "hello")["stream"].clone();
    let subscription = json!({"stream":stream,"keys":[{"key":format!("{}:chair",server.session),"from":seed.len(),"view":"rows"}]}).to_string();
    assert_eq!(
        server
            .request(
                "POST",
                "/api/subscribe",
                "Content-Type: application/json\r\n",
                &subscription,
                true
            )
            .0,
        200
    );
    let cached = page(&server, &path);
    let append = record("pair-next", "user", "text", "next")
        + &record("pair-new", "assistant", "text", "second");
    server.append(&append);
    let frame = loop {
        let frame = named_event(&mut live, "rows");
        if frame["end_offset"] == seed.len() + append.len() {
            break frame;
        }
    };
    let home = server.dir.join("home");
    let screen = |a: &str, b: &str| {
        format!(
            "Steering: {a}\nFollow-up: {b}\n\u{21b3} Option+Up to edit all queued messages\n\u{2500}\n\n\u{2500}\n"
        )
    };
    fs::write(home.join("screen"), screen("first", "second")).unwrap();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &server.session, "orchestrator", "pi").unwrap();
    drop(connection);
    let initial = ready_queue(&server, &["first", "second"]);
    fs::write(home.join("screen"), screen("second", "first")).unwrap();
    let refreshed = ready_queue(&server, &["second", "first"]);
    let pairs = json!({"merged":pair(&merged,"rows"),"cached":pair(&cached,"rows"),"sse":pair(&frame,"upsert")});
    let expected_pairs = pairs
        .as_object()
        .unwrap()
        .iter()
        .map(|(transport, entries)| {
            let entries = entries
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, value)| {
                    (
                        id.clone(),
                        json!({"actual":value["expected"],"expected":value["expected"]}),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            (transport.clone(), Value::Object(entries))
        })
        .collect::<serde_json::Map<_, _>>();
    assert_eq!(
        json!({"pairs":pairs,"initial":queue_modes(&initial),"refreshed":queue_modes(&refreshed)}),
        json!({
            "pairs":expected_pairs,
            "initial":[{"id":"queue-0","text":"first","body":"tree"},{"id":"queue-1","text":"second","body":"tree"}],
            "refreshed":[{"id":"queue-0","text":"second","body":"tree"},{"id":"queue-1","text":"first","body":"tree"}]
        }),
        "body must stay paired with each served row id"
    );
}
