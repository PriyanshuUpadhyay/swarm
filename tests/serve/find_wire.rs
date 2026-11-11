use super::*;

fn subscribe_rows(server: &Server, live: &mut BufReader<TcpStream>, from: u64) {
    let stream = named_event(live, "hello")["stream"]
        .as_str()
        .unwrap()
        .to_owned();
    let subscription = json!({"stream": stream, "keys": [{
        "key": format!("{}:chair", server.session), "from": from, "view": "rows"
    }]})
    .to_string();
    let (status, _, body) = server.request(
        "POST",
        "/api/subscribe",
        "Content-Type: application/json\r\n",
        &subscription,
        true,
    );
    assert_eq!(status, 200, "{body}");
}

#[test]
fn find_map_on_merged_page_and_cached_tail_matches_original_rows() {
    let server = Server::new();
    let log = include_str!("find-t1.jsonl");
    fs::write(&server.log, log).unwrap();
    let expected: Value = serde_json::from_str(include_str!("find-map.json")).unwrap();
    let path = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    let (status, _, body) = server.request("GET", &path, "", "", true);
    assert_eq!(status, 200, "{body}");
    let merged: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(merged["find"], expected);

    let mut live = server.sse();
    subscribe_rows(&server, &mut live, log.len() as u64);
    let (status, _, body) = server.request("GET", &path, "", "", true);
    assert_eq!(status, 200, "{body}");
    let cached: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(cached["find"], expected);
}

#[test]
fn find_map_on_public_rows_subscription_matches_the_upserted_rows() {
    let server = Server::new();
    let log = include_str!("find-t1.jsonl");
    let lines = log.split_inclusive('\n').collect::<Vec<_>>();
    let before = lines[..8].concat();
    fs::write(&server.log, &before).unwrap();
    let mut live = server.sse();
    subscribe_rows(&server, &mut live, before.len() as u64);
    // The original Edit results and remaining lines retain their exact byte offsets.
    server.append(&lines[8..].concat());
    let frame = named_event(&mut live, "rows");
    assert_eq!(frame["end_offset"], log.len());
    let upsert = frame["upsert"].as_array().unwrap();
    assert!(upsert.iter().any(|row| row["id"] == "e1:call@raw-1361-0"));
    let map: Value = serde_json::from_str(include_str!("find-map.json")).unwrap();
    let mut expected = serde_json::Map::new();
    for row in upsert {
        if let Some(parts) = map.get(row["id"].as_str().unwrap()) {
            expected.insert(row["id"].as_str().unwrap().to_owned(), parts.clone());
        }
        for step in row["steps"].as_array().into_iter().flatten() {
            if let Some(parts) = map.get(step["id"].as_str().unwrap()) {
                expected.insert(step["id"].as_str().unwrap().to_owned(), parts.clone());
            }
        }
    }
    assert_eq!(frame["find"], Value::Object(expected));
}
