// S10e-1 frozen RED tests for `POST /api/rows` (contract FIND-V1, owner ruling 2026-11-10).
// Expected values come only from the ORIGINAL browser matcher (find_original/*.json, copied byte for
// byte from the capture) or are written by hand here. They never come from new Rust output.
use super::*;
use std::collections::HashMap;
use std::thread;

/// Sends one POST with a body of any size. A server that refuses the body may close early and
/// reset the socket, so a second thread reads the answer while this one writes, and a failed
/// write only stops the writing.
fn post_rows(server: &Server, body: &[u8]) -> (u16, Value) {
    let mut stream = server.connection();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut reader = stream.try_clone().unwrap();
    let answer = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(n) = reader.read(&mut chunk) {
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..n]);
        }
        bytes
    });
    let head = format!(
        "POST /api/rows HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        server.port,
        server.token,
        body.len()
    );
    if stream.write_all(head.as_bytes()).is_ok() {
        for part in body.chunks(64 * 1024) {
            if stream.write_all(part).is_err() {
                break;
            }
        }
    }
    let response = String::from_utf8(answer.join().unwrap()).unwrap();
    let (headers, text) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no HTTP answer: {response:?}"));
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    let value = serde_json::from_str(text).unwrap_or(Value::String(text.to_owned()));
    (status, value)
}

fn request(query: &str, rows: Value, find: Value, full: Value) -> Value {
    json!({"version": 1, "query": query, "rows": rows, "find": find, "full": full})
}

/// The browser kept loaded text as `[id, {state, text}]` pairs. Only ready text is sent.
fn ready_text(entries: &Value) -> Value {
    let mut full = serde_json::Map::new();
    for pair in entries.as_array().into_iter().flatten() {
        if pair[1]["state"] == "ready" {
            full.insert(
                pair[0].as_str().unwrap().to_owned(),
                pair[1]["text"].clone(),
            );
        }
    }
    Value::Object(full)
}

fn original_cases() -> Vec<(String, Value)> {
    let parts = [
        include_str!("find_original/inputs-parts-0.json"),
        include_str!("find_original/inputs-parts-1.json"),
    ];
    let lines = include_str!("find_original/inputs-lines.json");
    let hand = include_str!("find_original/inputs-hand.json");
    let mut cases = Vec::new();
    for text in parts {
        let file: Value = serde_json::from_str(text).unwrap();
        let set = &file["set"];
        for variant in set["fulls"].as_array().unwrap() {
            for query in file["queries"].as_array().unwrap() {
                let id = format!(
                    "{}/{}/{}",
                    set["id"].as_str().unwrap(),
                    variant["id"].as_str().unwrap(),
                    query["id"].as_str().unwrap()
                );
                let body = request(
                    query["q"].as_str().unwrap(),
                    set["rows"].clone(),
                    file["find"].clone(),
                    ready_text(&variant["entries"]),
                );
                cases.push((id, body));
            }
        }
    }
    for text in [lines, hand] {
        let list: Value = serde_json::from_str(text).unwrap();
        for case in list.as_array().unwrap() {
            let body = request(
                case["query"].as_str().unwrap(),
                case["rows"].clone(),
                case["find"].clone(),
                ready_text(&case["full"]),
            );
            cases.push((case["id"].as_str().unwrap().to_owned(), body));
        }
    }
    cases
}

fn original_matches() -> HashMap<String, Value> {
    let files = [
        include_str!("find_original/matches-00.json"),
        include_str!("find_original/matches-01.json"),
        include_str!("find_original/matches-02.json"),
        include_str!("find_original/matches-03.json"),
        include_str!("find_original/matches-04.json"),
        include_str!("find_original/matches-05.json"),
        include_str!("find_original/matches-06.json"),
        include_str!("find_original/matches-07.json"),
    ];
    let mut expected = HashMap::new();
    for text in files {
        let list: Value = serde_json::from_str(text).unwrap();
        for case in list.as_array().unwrap() {
            let id = case["id"].as_str().unwrap().to_owned();
            assert!(expected.insert(id, case["matches"].clone()).is_none());
        }
    }
    expected
}

#[test]
fn post_rows_gives_the_original_matches_for_every_captured_case() {
    let cases = original_cases();
    let expected = original_matches();
    assert_eq!(cases.len(), 207);
    assert_eq!(expected.len(), 207);
    let server = Server::new();
    let mut wrong = Vec::new();
    for (id, body) in &cases {
        let (status, reply) = post_rows(&server, body.to_string().as_bytes());
        if status != 200 {
            wrong.push(format!("{id}: status {status} {reply}"));
        } else if reply["matches"] != expected[id] {
            wrong.push(format!(
                "{id}: got {} want {}",
                reply["matches"], expected[id]
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} cases differ; first three: {:#?}",
        wrong.len(),
        cases.len(),
        &wrong[..wrong.len().min(3)]
    );
}

#[test]
fn post_rows_answers_ready_full_text_in_the_versioned_json_shape() {
    let server = Server::new();
    // HAND value from FIND-V1: the preview says "miss", the ready full text holds two hits on line 1.
    let body = r#"{"version":1,"query":"needle","rows":[{"id":"p","kind":"assistant","text":"","order":[1,0],"output":{"id":"p:out","text":"miss","full_lines":1,"full_bytes":4,"hidden_lines":0}}],"find":{"p":[{"f":"output.text","p":"p:out"}]},"full":{"p:out":"before\nneedle needle\nafter"}}"#;
    let (status, reply) = post_rows(&server, body.as_bytes());
    assert_eq!(status, 200, "{reply}");
    assert_eq!(
        reply,
        json!({"version": 1, "matches": [
            {"id": "p", "fold": null, "offset": 7, "occurrence": 0,
             "preview": "p:out", "line": 1, "lineOccurrence": 0},
            {"id": "p", "fold": null, "offset": 14, "occurrence": 1,
             "preview": "p:out", "line": 1, "lineOccurrence": 1}]})
    );
}

#[test]
fn post_rows_takes_a_one_mib_snapshot_that_the_other_routes_would_refuse() {
    let server = Server::new();
    // The route has its own 16 MiB limit; every other route keeps 64 KiB. The only hit sits at the very end.
    let mut text = "x".repeat(1_048_570);
    text.push_str("needle");
    let body = request(
        "needle",
        json!([{"id": "p", "kind": "assistant", "text": "", "order": [1, 0],
                "output": {"id": "p:out", "text": "miss", "full_lines": 1, "full_bytes": 4, "hidden_lines": 0}}]),
        json!({"p": [{"f": "output.text", "p": "p:out"}]}),
        json!({"p:out": text}),
    )
    .to_string();
    assert!(body.len() > 1_048_576 && body.len() < 1_049_000);
    let (status, reply) = post_rows(&server, body.as_bytes());
    assert_eq!(
        status,
        200,
        "{}",
        reply.to_string().chars().take(200).collect::<String>()
    );
    assert_eq!(
        reply["matches"],
        json!([{"id": "p", "fold": null, "offset": 1_048_570, "occurrence": 0,
                "preview": "p:out", "line": 0, "lineOccurrence": 0}])
    );
}

/// A valid snapshot of exactly `size` bytes whose one hit is the word at the end of the full text.
/// The other routes already answer 413 above 64 KiB, so only a body at the limit can tell them apart.
fn snapshot_of(size: usize) -> (String, usize) {
    let make = |length: usize| {
        let mut text = "x".repeat(length - 6);
        text.push_str("needle");
        request(
            "needle",
            json!([{"id": "p", "kind": "assistant", "text": "", "order": [1, 0],
                    "output": {"id": "p:out", "text": "miss", "full_lines": 1, "full_bytes": 4, "hidden_lines": 0}}]),
            json!({"p": [{"f": "output.text", "p": "p:out"}]}),
            json!({"p:out": text}),
        )
        .to_string()
    };
    let length = size - make(6).len() + 6;
    let body = make(length);
    assert_eq!(body.len(), size);
    (body, length - 6)
}

#[test]
fn post_rows_takes_a_body_of_exactly_sixteen_mib_and_refuses_one_byte_more_with_413() {
    let server = Server::new();
    let limit = 16 * 1024 * 1024;
    let (body, offset) = snapshot_of(limit);
    let (status, reply) = post_rows(&server, body.as_bytes());
    assert_eq!(
        status,
        200,
        "{}",
        reply.to_string().chars().take(200).collect::<String>()
    );
    assert_eq!(
        reply["matches"],
        json!([{"id": "p", "fold": null, "offset": offset, "occurrence": 0,
                "preview": "p:out", "line": 0, "lineOccurrence": 0}])
    );
    let (body, _) = snapshot_of(limit + 1);
    let (status, reply) = post_rows(&server, body.as_bytes());
    assert_eq!(
        status,
        413,
        "{}",
        reply.to_string().chars().take(200).collect::<String>()
    );
    assert!(
        reply["error"].as_str().is_some_and(|text| !text.is_empty()),
        "{reply}"
    );
}

#[test]
fn post_rows_refuses_a_version_other_than_one_with_400() {
    let server = Server::new();
    let mut body = request("alpha", json!([]), json!({}), json!({}));
    body["version"] = json!(2);
    let (status, reply) = post_rows(&server, body.to_string().as_bytes());
    assert_eq!(status, 400, "{reply}");
    assert!(
        reply["error"].as_str().is_some_and(|text| !text.is_empty()),
        "{reply}"
    );
}
