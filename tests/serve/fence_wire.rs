use super::*;

// S6C-FENCES-V1 T1. Expected Code objects are HAND contract values and
// ORIGINAL S4/S6a values frozen before implementation, not candidate output.
#[test]
fn fence_wire_pages_and_live_append_keep_full_code_entries() {
    let cases: Value = serde_json::from_str(include_str!("../fixtures/s6c/fences.json")).unwrap();
    let cases = cases.as_array().unwrap();
    let markdown = cases
        .iter()
        .map(|case| case["markdown"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n\n");
    let expected = cases
        .iter()
        .map(|case| case["code"].clone())
        .collect::<Vec<_>>();
    let record = |id: &str, kind: &str, text: &str| {
        json!({"type":kind,"uuid":id,"message":{"content":[{"type":"text","text":text}]}})
            .to_string()
            + "\n"
    };
    let server = Server::new();
    // The incremental transcript reader needs more than its 31-record seed.
    let mut seed = (0..32)
        .map(|i| record(&format!("fence-pad-{i}"), "user", "pad"))
        .collect::<String>();
    seed.push_str(&record("fence-page", "assistant", &markdown));
    fs::write(&server.log, &seed).unwrap();
    let path = format!("/api/rows?session={}&agent=chair&tail=200", server.session);
    let read_page = || {
        let (status, _, body) = server.request("GET", &path, "", "", true);
        assert_eq!(status, 200, "{body}");
        serde_json::from_str::<Value>(&body).unwrap()
    };
    // Get entries through row ids, including fold steps, so an unrelated map
    // value cannot satisfy the assert while the painted row has no entry.
    fn entries(rows: &Value, display: &Value, text: &str) -> Value {
        let mut pending = rows.as_array().unwrap().iter().collect::<Vec<_>>();
        while let Some(row) = pending.pop() {
            if let Some(steps) = row["steps"].as_array() {
                pending.extend(steps);
            }
            if row["text"] == text {
                return display[row["id"].as_str().unwrap()]["blocks"].clone();
            }
        }
        Value::Null
    }
    let page = read_page();
    let page_codes = entries(&page["rows"], &page["display"], &markdown);
    let mut live = server.sse();
    let stream = named_event(&mut live, "hello")["stream"].clone();
    let subscription = json!({"stream":stream,"keys":[{
        "key":format!("{}:chair",server.session),"from":seed.len(),"view":"rows"
    }]})
    .to_string();
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
    let cached = read_page();
    let cached_codes = entries(&cached["rows"], &cached["display"], &markdown);
    // A user turn separates two equal answers, so the live row is a new row.
    let append =
        record("fence-next", "user", "next") + &record("fence-live", "assistant", &markdown);
    server.append(&append);
    let frame = loop {
        let frame = named_event(&mut live, "rows");
        if frame["end_offset"] == seed.len() + append.len() {
            break frame;
        }
    };
    let live_codes = entries(&frame["upsert"], &frame["display"], &markdown);
    // Run page, cached-tail and live paths before the one whole-object assert.
    assert_eq!(
        json!({"page":page_codes,"cached":cached_codes,"live":live_codes}),
        json!({"page":expected,"cached":expected,"live":expected})
    );
}
