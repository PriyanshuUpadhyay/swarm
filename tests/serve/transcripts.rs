// Tests for transcripts.

use super::*;

#[test]
fn snapshot_transcript_paging_and_static_paths_run_on_the_real_server() {
    let server = Server::new();
    let (status, _, body) = server.request("GET", "/api/snapshot", "", "", true);
    assert_eq!(status, 200);
    let snapshot: Value = serde_json::from_str(&body).unwrap();
    assert!(snapshot["version"].as_u64().is_some());
    assert_eq!(snapshot["sessions"][0]["id"], server.session);
    assert!(snapshot["agents"].is_object());
    assert!(snapshot["sessions"][0]["project"].is_string());
    assert_eq!(
        snapshot["sessions"][0]["workspace"],
        env!("CARGO_MANIFEST_DIR")
    );
    let path = format!(
        "/api/transcript?session={}&agent=chair&tail=2000",
        server.session
    );
    let (status, _, body) = server.request("GET", &path, "", "", true);
    assert_eq!(status, 200, "{body}");
    let transcript: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(transcript["page"]["start_offset"], 0);
    assert_eq!(
        transcript["page"]["end_offset"],
        fs::metadata(&server.log).unwrap().len()
    );
    assert!(transcript["events"].as_array().unwrap().len() > 5);
    let path = format!(
        "/api/transcript?session={}&agent=worker&tail=1",
        server.session
    );
    let (_, _, body) = server.request("GET", &path, "", "", true);
    let last: Value = serde_json::from_str(&body).unwrap();
    let offset = last["page"]["start_offset"].as_u64().unwrap();
    assert!(offset > 0);
    let path = format!("{path}&before={offset}");
    let (status, _, body) = server.request("GET", &path, "", "", true);
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["page"]["end_offset"],
        offset
    );
    assert_eq!(
        server
            .request(
                "GET",
                &format!(
                    "/api/transcript?session={}&agent=chair&tail=0",
                    server.session
                ),
                "",
                "",
                true
            )
            .0,
        400
    );
    assert_eq!(
        server
            .request(
                "GET",
                "/api/transcript?session=unknown&agent=chair&tail=1",
                "",
                "",
                true
            )
            .0,
        404
    );
    assert_eq!(
        server
            .request(
                "GET",
                &format!(
                    "/api/transcript?session={}&agent=absent&tail=1",
                    server.session
                ),
                "",
                "",
                true
            )
            .0,
        404
    );
    let (status, _, body) = server.request("GET", "/", "", "", false);
    assert_eq!(status, 200);
    assert_eq!(body, "<h1>pilot</h1>");
    assert_eq!(server.request("GET", "/app.js", "", "", false).0, 200);
    // A chat route is the page itself, so a reload of `/chat/<id>` works.
    let (status, _, body) = server.request("GET", "/chat/01a13ee5", "", "", false);
    assert_eq!((status, body.as_str()), (200, "<h1>pilot</h1>"));
    assert_eq!(server.request("GET", "/escape.jsonl", "", "", false).0, 404);
    assert_eq!(
        server.request("GET", "/../chair.jsonl", "", "", false).0,
        404
    );
    assert_eq!(
        server
            .request("GET", "/%2e%2e/chair.jsonl", "", "", false)
            .0,
        404
    );
}

#[test]
fn shared_live_followers_send_complete_line_offsets_within_one_second() {
    let server = Server::new();
    let mut first = server.sse();
    let first_id = named_event(&mut first, "hello")["stream"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut second = server.sse();
    let second_id = named_event(&mut second, "hello")["stream"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = fs::metadata(&server.log).unwrap().len();
    server.subscribe(&first_id, before);
    server.subscribe(&second_id, before);
    let line = "{\"type\":\"user\",\"uuid\":\"live-1\",\"message\":{\"content\":\"live synthetic hello\"}}\n";
    let started = Instant::now();
    server.append(line);
    let first_live = named_event(&mut first, "transcript");
    let second_live = named_event(&mut second, "transcript");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(first_live, second_live);
    assert_eq!(first_live["key"], format!("{}:chair", server.session));
    assert_eq!(first_live["end_offset"], before + line.len() as u64);
    assert_eq!(first_live["events"][0]["text"], "live synthetic hello");
    let partial = "{\"type\":\"user\",\"uuid\":\"live-2\",\"message\":{\"content\":\"partial";
    server.append(partial);
    std::thread::sleep(Duration::from_millis(250));
    let finish = " line\"}}\n";
    server.append(finish);
    let complete = named_event(&mut first, "transcript");
    assert_eq!(complete["events"][0]["text"], "partial line");
    assert_eq!(
        complete["end_offset"],
        before + (line.len() + partial.len() + finish.len()) as u64
    );
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET cwd = ?1 WHERE id = ?2",
            (server.dir.to_string_lossy().as_ref(), &server.session),
        )
        .unwrap();
    let changed = named_event(&mut first, "snapshot");
    assert!(changed["version"].as_u64().unwrap() > 1);
    let body = json!({"stream": first_id, "keys": []}).to_string();
    assert_eq!(
        server
            .request(
                "POST",
                "/api/subscribe",
                "Content-Type: application/json\r\n",
                &body,
                true
            )
            .0,
        200
    );
    drop(second);
    assert_eq!(
        server
            .request(
                "POST",
                "/api/subscribe",
                "Content-Type: application/json\r\n",
                "{\"stream\":\"unknown\",\"keys\":[]}",
                true
            )
            .0,
        404
    );
}

#[test]
fn row_routes_guard_input_cap_output_and_stream_stable_ids() {
    let server = Server::new();
    let call = json!({"type":"assistant","uuid":"row-call","message":{"content":[{"type":"tool_use","id":"output-call","name":"Bash","input":{"command":"print"}}]}}).to_string()+"\n";
    fs::write(&server.log, &call).unwrap();
    let page_path = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    for path in [
        &page_path,
        "/api/tool-output?session=unknown&agent=chair&row=raw-0-0:output",
    ] {
        assert_eq!(server.request("GET", path, "", "", false).0, 401);
        assert_eq!(
            server
                .request("GET", path, "Origin: https://evil.test\r\n", "", true)
                .0,
            403
        );
        let mut stream = server.connection();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: evil.test\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n", server.token).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 403"));
    }
    assert_eq!(
        server
            .request(
                "GET",
                &page_path.replace("tail=200", "tail=0"),
                "",
                "",
                true
            )
            .0,
        400
    );
    let (status, _, body) = server.request("GET", &page_path, "", "", true);
    assert_eq!(status, 200, "{body}");
    let page: Value = serde_json::from_str(&body).unwrap();
    let id = page["rows"][0]["id"].clone();
    let mut live = server.sse();
    let stream_id = named_event(&mut live, "hello")["stream"]
        .as_str()
        .unwrap()
        .to_owned();
    let subscription = json!({"stream":stream_id,"keys":[{"key":format!("{}:chair",server.session),"from":call.len(),"view":"rows"}]}).to_string();
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
    let full = (0..10000)
        .map(|index| format!("output line {index}\n"))
        .collect::<String>();
    let result = json!({"type":"user","uuid":"row-output","message":{"content":[{"type":"tool_result","tool_use_id":"output-call","content":full}]}}).to_string()+"\n";
    server.append(&result);
    let frame = named_event(&mut live, "rows");
    assert_eq!(frame["upsert"][0]["id"], id);
    assert_eq!(frame["upsert"][0]["tool"]["state"], "finished");
    assert_eq!(frame["upsert"][0]["tool"]["output"]["full_lines"], 10000);
    assert!(frame.to_string().len() < 20000);
    let locator = frame["upsert"][0]["tool"]["output"]["id"].as_str().unwrap();
    let output_path = format!(
        "/api/tool-output?session={}&agent=chair&row={locator}",
        server.session
    );
    let (status, _, body) = server.request("GET", &output_path, "", "", true);
    assert_eq!(status, 200, "{body}");
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["text"], full);
    assert_eq!(
        server
            .request(
                "GET",
                &output_path.replace(locator, "raw-1-0:output"),
                "",
                "",
                true
            )
            .0,
        404
    );
}

#[test]
fn a_pi_chair_log_reads_as_rows() {
    let server = Server::new();
    let log = server.dir.join("pi.jsonl");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("packages/transcript/src/fixtures/pi-chat.jsonl"),
        &log,
    )
    .unwrap();
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    swarm::store::set_chair(&connection, &server.session, Some(("pi", "s1"))).unwrap();
    swarm::store::set_chair_log(&connection, &server.session, &log).unwrap();
    let path = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    let (status, _, body) = server.request("GET", &path, "", "", true);
    assert_eq!(status, 200, "{body}");
    for text in [
        "Run the tests",
        "One test fails.",
        "cargo test",
        "overloaded",
    ] {
        assert!(body.contains(text), "{text} missing from {body}");
    }
}

/// pi has no hook to report its log, so the server finds it by the session id that `launch` gave
/// pi, `<session>-<agent>` (ADR 0086).
#[test]
fn a_pi_log_is_found_by_its_session_id() {
    let server = Server::new();
    let sessions = server.dir.join("home/.pi/agent/sessions/--work--");
    fs::create_dir_all(&sessions).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("packages/transcript/src/fixtures/pi-chat.jsonl");
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    let chair = format!("{}-orchestrator", server.session);
    swarm::store::set_chair(&connection, &server.session, Some(("pi", &chair))).unwrap();
    swarm::store::add_agent(&connection, &server.session, "piseat", "chat").unwrap();
    swarm::store::set_provider(&connection, &server.session, "piseat", "pi").unwrap();
    for agent in ["chair", "piseat"] {
        let path = format!(
            "/api/rows?session={}&agent={agent}&tail=200",
            server.session
        );
        let (status, _, body) = server.request("GET", &path, "", "", true);
        assert_eq!(status, 404, "{agent} before its log: {body}");
    }
    for id in [chair, format!("{}-piseat", server.session)] {
        fs::copy(
            &fixture,
            sessions.join(format!("2026-10-14T10-00-00-000Z_{id}.jsonl")),
        )
        .unwrap();
    }
    for agent in ["chair", "piseat"] {
        let path = format!(
            "/api/rows?session={}&agent={agent}&tail=200",
            server.session
        );
        let (status, _, body) = server.request("GET", &path, "", "", true);
        assert_eq!(status, 200, "{agent}: {body}");
        assert!(body.contains("One test fails."), "{agent}: {body}");
    }
}
