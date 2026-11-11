use std::cell::RefCell;
thread_local! { static RESULTS: RefCell<serde_json::Map<String,Value>> = RefCell::default(); }
use std::io::ErrorKind;

fn input(name: &str) -> Vec<u8> {
    frozen::bytes(&format!("inputs/{name}.jsonl"))
}
struct Cap {
    case: String,
    session: String,
}
impl Cap {
    fn new(case: &str, server: &Server) -> Self {
        Self {
            case: case.into(),
            session: server.session.clone(),
        }
    }
    fn put(&self, name: &str, text: &str) {
        let value = frozen::parse(name, &text.replace(&self.session, "SESSION"));
        RESULTS.with(|all| {
            all.borrow_mut()
                .insert(format!("{}/{name}", self.case), value);
        });
    }
}
fn get(server: &Server, path: &str) -> String {
    let (status, _, body) = server.request("GET", path, "", "", true);
    format!("{status}\n{body}")
}

fn ids(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                if key == "id"
                    && let Some(id) = item.as_str()
                    && (id.ends_with(":output") || id.ends_with(":diff"))
                {
                    found.push(id.to_owned());
                }
                ids(item, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| ids(item, found)),
        _ => {}
    }
}

fn collect(reader: &mut BufReader<TcpStream>, quiet: Duration) -> Vec<Value> {
    reader.get_ref().set_read_timeout(Some(quiet)).unwrap();
    let (mut found, mut name, mut line) = (Vec::new(), String::new(), String::new());
    loop {
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if !line.ends_with('\n') {
                    continue;
                }
                let text = std::mem::take(&mut line);
                if let Some(value) = text.strip_prefix("event: ") {
                    name = value.trim().to_owned();
                } else if let Some(value) = text.strip_prefix("data: ")
                    && name != "hello"
                    && name != "snapshot"
                {
                    found.push(json!({"event": name, "data": serde_json::from_str::<Value>(value.trim()).unwrap()}));
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break;
            }
            Err(error) => panic!("{error}"),
        }
    }
    found
}

fn open(server: &Server, from: u64, rows: bool) -> BufReader<TcpStream> {
    let mut stream = server.sse();
    let id = named_event(&mut stream, "hello")["stream"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut entry = json!({"key": format!("{}:chair", server.session), "from": from});
    if rows {
        entry["view"] = json!("rows");
    }
    let (status, _, body) = server.request(
        "POST",
        "/api/subscribe",
        "Content-Type: application/json\r\n",
        &json!({"stream": id, "keys": [entry]}).to_string(),
        true,
    );
    assert_eq!(status, 200, "{body}");
    stream
}

fn pages(name: &str, deep: bool) {
    let server = Server::new();
    fs::write(&server.log, input(name)).unwrap();
    let cap = Cap::new(name, &server);
    let base = format!("session={}&agent=chair", server.session);
    let full = get(&server, &format!("/api/rows?{base}&tail=2000"));
    cap.put("rows-full.txt", &full);
    cap.put(
        "raw-full.txt",
        &get(&server, &format!("/api/transcript?{base}&tail=2000")),
    );
    let page: Value = serde_json::from_str(full.split_once('\n').unwrap().1).unwrap_or(Value::Null);
    let mut found = Vec::new();
    ids(&page, &mut found);
    for (index, id) in found.iter().enumerate() {
        cap.put(
            &format!("output-{index:02}.txt"),
            &format!(
                "{id}\n{}",
                get(&server, &format!("/api/tool-output?{base}&row={id}"))
            ),
        );
    }
    if !deep {
        return;
    }
    for tail in [1, 3] {
        cap.put(
            &format!("rows-tail-{tail}.txt"),
            &get(&server, &format!("/api/rows?{base}&tail={tail}")),
        );
    }
    let mut before: Option<u64> = None;
    for step in 0..40 {
        let suffix = before.map_or(String::new(), |offset| format!("&before={offset}"));
        let body = get(&server, &format!("/api/rows?{base}&tail=2{suffix}"));
        cap.put(&format!("rows-back-{step:02}.txt"), &body);
        cap.put(
            &format!("raw-back-{step:02}.txt"),
            &get(&server, &format!("/api/transcript?{base}&tail=2{suffix}")),
        );
        let start = serde_json::from_str::<Value>(body.split_once('\n').unwrap().1)
            .ok()
            .and_then(|page| page["page"]["start_offset"].as_u64());
        match start {
            Some(0) | None => break,
            Some(offset) => before = Some(offset),
        }
    }
    for (label, path) in [
        ("tail-0", format!("/api/rows?{base}&tail=0")),
        ("raw-tail-0", format!("/api/transcript?{base}&tail=0")),
        (
            "unknown-session",
            "/api/rows?session=unknown&agent=chair&tail=1".to_owned(),
        ),
        (
            "unknown-agent",
            format!("/api/rows?session={}&agent=absent&tail=1", server.session),
        ),
        (
            "unknown-output",
            format!("/api/tool-output?{base}&row=raw-1-0:output"),
        ),
    ] {
        cap.put(&format!("error-{label}.txt"), &get(&server, &path));
    }
}

fn lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .map(<[u8]>::to_vec)
        .collect()
}

fn frames(cap: &Cap, label: &str, rows: &mut BufReader<TcpStream>, raw: &mut BufReader<TcpStream>) {
    let quiet = Duration::from_millis(900);
    let value = json!({"rows": collect(rows, quiet), "raw": collect(raw, quiet)});
    cap.put(
        &format!("{label}.json"),
        &serde_json::to_string_pretty(&value).unwrap(),
    );
}

fn follow() {
    let all = input("claude-chair-run");
    let parts = lines(&all);
    let server = Server::new();
    let head: Vec<u8> = parts[..3].concat();
    fs::write(&server.log, &head).unwrap();
    let cap = Cap::new("follow-claude-chair-run", &server);
    let base = format!("session={}&agent=chair", server.session);
    let from = head.len() as u64;
    let (mut rows, mut raw) = (open(&server, from, true), open(&server, from, false));
    frames(&cap, "step-00-initial", &mut rows, &mut raw);
    cap.put(
        "page-00-before-result.txt",
        &get(&server, &format!("/api/rows?{base}&tail=2000")),
    );
    // The tool result of line 4 arrives split in two writes: nothing may show until the line is whole.
    let result = &parts[3];
    let cut = result.len() / 2;
    server.append(std::str::from_utf8(&result[..cut]).unwrap());
    frames(&cap, "step-01-partial-result", &mut rows, &mut raw);
    server.append(std::str::from_utf8(&result[cut..]).unwrap());
    frames(&cap, "step-02-late-result", &mut rows, &mut raw);
    cap.put(
        "page-01-after-result.txt",
        &get(&server, &format!("/api/rows?{base}&tail=2000")),
    );
    for (index, line) in parts.iter().enumerate().skip(4) {
        server.append(std::str::from_utf8(line).unwrap());
        frames(
            &cap,
            &format!("step-{:02}-line-{}", index - 1, index + 1),
            &mut rows,
            &mut raw,
        );
    }
    cap.put(
        "page-02-final.txt",
        &get(&server, &format!("/api/rows?{base}&tail=2000")),
    );
    cap.put(
        "raw-02-final.txt",
        &get(&server, &format!("/api/transcript?{base}&tail=2000")),
    );
}

fn overlap() {
    let all = input("claude-chair-run");
    let parts = lines(&all);
    let offset = |count: usize| parts[..count].iter().map(Vec::len).sum::<usize>() as u64;
    for (label, from) in [
        ("from-0", 0),
        ("from-line-5", offset(4)),
        ("from-end-plus-1000", all.len() as u64 + 1000),
    ] {
        let server = Server::new();
        fs::write(&server.log, &all).unwrap();
        let cap = Cap::new(&format!("overlap-{label}"), &server);
        let (mut rows, mut raw) = (open(&server, from, true), open(&server, from, false));
        frames(&cap, "step-00-subscribe", &mut rows, &mut raw);
        let extra =
            "{\"type\":\"user\",\"message\":{\"content\":\"appended after the overlap\"}}\n";
        server.append(extra);
        frames(&cap, "step-01-append", &mut rows, &mut raw);
        if label == "from-line-5" {
            // A second client that joins the same key from earlier text: the recovery path of a running follower.
            let (mut rows2, mut raw2) = (open(&server, 0, true), open(&server, 0, false));
            frames(&cap, "step-02-second-client-from-0", &mut rows2, &mut raw2);
        }
    }
}

pub fn all() {
    for name in [
        "claude-chair-run",
        "claude-edit",
        "crlf-claude",
        "append-unterminated",
        "long-line-claude",
        "giant-output-claude",
        "guard-claude-invalid-json",
        "guard-claude-invalid-utf8",
        "guard-claude-unknown-log",
        "guard-claude-non-ascii-escape",
        "claude-input-depth-200",
        "claude-input-depth-255",
        "claude-input-depth-256",
        "numbers-input-1e400",
        "numbers-input-cost-decimal",
        "numbers-input-cost-zero",
        "numbers-input-duration-negzero",
        "numbers-input-negzero-whole-float",
        "numbers-input-u64-plus-1",
    ] {
        pages(name, name == "claude-chair-run" || name == "claude-edit");
    }
    follow();
    overlap();
}

pub fn result() -> Value {
    RESULTS.with(|all| Value::Object(std::mem::take(&mut *all.borrow_mut())))
}
