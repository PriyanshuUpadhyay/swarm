// Shared real-route journey. Expected bytes come only from the OLD capture.
use std::io::ErrorKind;
use std::path::Path;
thread_local! {
    static CAPTURE: std::cell::RefCell<serde_json::Map<String, Value>> = std::cell::RefCell::new(serde_json::Map::new());
}
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
        CAPTURE.with_borrow_mut(|values| {
            values.insert(format!("{}/{name}", self.case), value);
        });
    }
}
fn result() -> Value {
    CAPTURE.with_borrow(|values| Value::Object(values.clone()))
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
                    && name != "ping"
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
    let server = server();
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
    for tail in [1, 3, 4, 8, 11] {
        cap.put(
            &format!("rows-tail-{tail}.txt"),
            &get(&server, &format!("/api/rows?{base}&tail={tail}")),
        );
        cap.put(
            &format!("raw-tail-{tail}.txt"),
            &get(&server, &format!("/api/transcript?{base}&tail={tail}")),
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
    let all = input("codex-kernel");
    let parts = lines(&all);
    let server = server();
    let head: Vec<u8> = parts[..8].concat();
    fs::write(&server.log, &head).unwrap();
    let cap = Cap::new("follow-codex-kernel", &server);
    let base = format!("session={}&agent=chair", server.session);
    // Starting at zero lets the live translator see the pending question call.
    let (mut rows, mut raw) = (open(&server, 0, true), open(&server, 0, false));
    frames(&cap, "step-00-pending", &mut rows, &mut raw);
    cap.put(
        "page-00-pending.txt",
        &get(&server, &format!("/api/rows?{base}&tail=2000")),
    );
    cap.put(
        "page-01-replay-without-question.txt",
        &get(&server, &format!("/api/rows?{base}&tail=2000&before=1252")),
    );
    let result = &parts[8];
    let cut = result.len() / 2;
    server.append(std::str::from_utf8(&result[..cut]).unwrap());
    frames(&cap, "step-01-partial-output", &mut rows, &mut raw);
    server.append(std::str::from_utf8(&result[cut..]).unwrap());
    frames(&cap, "step-02-complete-output", &mut rows, &mut raw);
    for (index, line) in parts.iter().enumerate().skip(9) {
        server.append(std::str::from_utf8(line).unwrap());
        frames(
            &cap,
            &format!("step-{:02}-line-{}", index - 6, index + 1),
            &mut rows,
            &mut raw,
        );
    }
    let final_page = get(&server, &format!("/api/rows?{base}&tail=2000"));
    cap.put("page-02-final.txt", &final_page);
    let page: Value = serde_json::from_str(final_page.split_once('\n').unwrap().1).unwrap();
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
    cap.put(
        "page-03-back.txt",
        &get(&server, &format!("/api/rows?{base}&tail=2&before=2771")),
    );
    cap.put(
        "raw-02-final.txt",
        &get(&server, &format!("/api/transcript?{base}&tail=2000")),
    );
    drop(rows);
    drop(raw);
    cap.put(
        "page-04-reopen.txt",
        &get(&server, &format!("/api/rows?{base}&tail=2000")),
    );
}

fn overlap() {
    let all = input("codex-kernel");
    let parts = lines(&all);
    let offset = |count: usize| parts[..count].iter().map(Vec::len).sum::<usize>() as u64;
    for (label, from) in [
        ("from-0", 0),
        ("from-line-5", offset(4)),
        ("from-end-plus-1000", all.len() as u64 + 1000),
    ] {
        let server = server();
        fs::write(&server.log, &all).unwrap();
        let cap = Cap::new(&format!("overlap-{label}"), &server);
        let (mut rows, mut raw) = (open(&server, from, true), open(&server, from, false));
        frames(&cap, "step-00-subscribe", &mut rows, &mut raw);
        let extra = "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"appended after the overlap\"}]}}\n";
        server.append(extra);
        frames(&cap, "step-01-append", &mut rows, &mut raw);
        if label == "from-line-5" {
            // A second client that joins the same key from earlier text: the recovery path of a running follower.
            let (mut rows2, mut raw2) = (open(&server, 0, true), open(&server, 0, false));
            frames(&cap, "step-02-second-client-from-0", &mut rows2, &mut raw2);
        }
    }
}

fn server() -> Server {
    let server = Server::new();
    bind(&server, "chair", &server.log);
    server
}
fn bind(server: &Server, agent: &str, path: &Path) {
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    if agent == "chair" {
        connection
            .execute(
                "UPDATE session SET chair_provider='codex',chair_log=?1 WHERE id=?2",
                (path.to_string_lossy().as_ref(), &server.session),
            )
            .unwrap();
    } else {
        connection
            .execute(
                "UPDATE agent SET provider='codex',log=?1 WHERE session_id=?2 AND id=?3",
                (path.to_string_lossy().as_ref(), &server.session, agent),
            )
            .unwrap();
    }
}
fn account_paths() {
    for (name, home) in [
        ("native", ".codex"),
        ("override", "codex-home"),
        ("profile", ".codex-work"),
        ("managed", ".swarm/accounts/codex/work"),
    ] {
        for agent in ["chair", "worker"] {
            let server = server();
            let path = server.dir.join("home").join(home).join("sessions/2026/11/09/rollout-2026-11-09T00-00-00-00000000-0000-7000-8000-000000000001.jsonl");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, input("codex-kernel")).unwrap();
            bind(&server, agent, &path);
            let cap = Cap::new(&format!("account-{name}-{agent}"), &server);
            let base = format!("session={}&agent={agent}", server.session);
            for (label, route) in [("rows", "rows"), ("raw", "transcript")] {
                cap.put(
                    &format!("{label}-full.txt"),
                    &get(&server, &format!("/api/{route}?{base}&tail=2000")),
                );
            }
        }
    }
}
fn saved() {
    for name in [
        "codex-chair-run",
        "codex-exec-edit",
        "codex-kernel",
        "kernel-prefix-1752",
        "kernel-prefix-2771",
        "codex-yield",
        "codex-tool-search",
        "codex-numbers-nodeep",
        "codex-depth254",
        "codex-negzero-whole",
        "codex-duration-negzero",
        "question-cancel",
    ] {
        pages(
            name,
            matches!(
                name,
                "codex-chair-run"
                    | "codex-exec-edit"
                    | "codex-kernel"
                    | "kernel-prefix-1752"
                    | "kernel-prefix-2771"
                    | "codex-yield"
            ),
        );
    }
}

fn all() {
    saved();
    let prefixes = ["codex-", "kernel-", "question-cancel/"];
    assert_eq!(result(), frozen::expected(&prefixes));
    follow();
    overlap();
    account_paths();
    origins();
}

fn origins() {
    for (name, from) in [
        ("codex-kernel", 1752),
        ("codex-kernel", 2549),
        ("codex-yield", 1143),
        ("codex-yield", 1887),
    ] {
        let server = server();
        fs::write(&server.log, input(name)).unwrap();
        let cap = Cap::new(&format!("origin-{name}-{from}"), &server);
        let (mut rows, mut raw) = (open(&server, from, true), open(&server, from, false));
        frames(&cap, "initial", &mut rows, &mut raw);
    }
    for (label, head, from) in [
        ("prefix-2771", 2771, 0),
        ("pending-origin-1752", 1752, 1752),
    ] {
        let server = server();
        let bytes = input("codex-kernel");
        fs::write(&server.log, &bytes[..head]).unwrap();
        let cap = Cap::new(label, &server);
        let (mut rows, mut raw) = (open(&server, from, true), open(&server, from, false));
        frames(&cap, "initial", &mut rows, &mut raw);
        let base = format!("session={}&agent=chair", server.session);
        cap.put(
            "page-replay.txt",
            &get(&server, &format!("/api/rows?{base}&tail=2000&before=1252")),
        );
        server.append(std::str::from_utf8(&bytes[head..]).unwrap());
        frames(&cap, "append", &mut rows, &mut raw);
        frames(&cap, "quiet", &mut rows, &mut raw);
    }
}
