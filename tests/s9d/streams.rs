struct SseStream {
    reader: BufReader<TcpStream>,
    pending: Vec<Value>,
    check: Option<RowCheck>,
}

fn open_pair(
    s: &Server,
    from: u64,
    cap: &Cap,
    name: &str,
    seed: Option<&str>,
) -> (SseStream, SseStream) {
    let expected: Value =
        serde_json::from_slice(&frozen::bytes(&format!("out-1/{}/{name}.json", cap.case))).unwrap();
    let mut raw = SseStream {
        reader: open(s, from, false),
        pending: Vec::new(),
        check: None,
    };
    // Drain raw first so the final older-offset subscription owns a rows watcher (ruling 1942).
    raw.pending = collect(&mut raw, expected["raw"].as_array().unwrap());
    let rows = SseStream {
        reader: open(s, from, true),
        pending: Vec::new(),
        check: Some(RowCheck::new(cap, name, seed)),
    };
    (rows, raw)
}
