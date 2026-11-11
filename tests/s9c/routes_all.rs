include!("../serve/legacy_body.rs");
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
    CAPTURE.with_borrow(|values| without_body(Value::Object(values.clone())))
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

fn page_values(
    cap: &Cap,
    server: &Server,
    base: &str,
    name: &str,
    tail: u16,
    before: Option<u64>,
) -> Value {
    let suffix = before.map_or(String::new(), |offset| format!("&before={offset}"));
    let body = get(server, &format!("/api/rows?{base}&tail={tail}{suffix}"));
    cap.put(&format!("rows-{name}.txt"), &body);
    cap.put(
        &format!("raw-{name}.txt"),
        &get(
            server,
            &format!("/api/transcript?{base}&tail={tail}{suffix}"),
        ),
    );
    serde_json::from_str(body.split_once('\n').unwrap().1).unwrap()
}
fn outputs(cap: &Cap, server: &Server, base: &str, page: &Value) {
    let mut found = Vec::new();
    ids(page, &mut found);
    for (index, id) in found.iter().enumerate() {
        cap.put(
            &format!("output-{index:02}.txt"),
            &format!(
                "{id}\n{}",
                get(server, &format!("/api/tool-output?{base}&row={id}"))
            ),
        );
    }
}
fn pages(name: &str) {
    let server = server();
    fs::write(&server.log, input(name)).unwrap();
    let cap = Cap::new(name, &server);
    let base = format!("session={}&agent=chair", server.session);
    let full = page_values(&cap, &server, &base, "full", 2000, None);
    outputs(&cap, &server, &base, &full);
    if full.get("rows").is_none() {
        return;
    }
    for tail in [1, 3, 4, 8, 11, 16] {
        page_values(&cap, &server, &base, &format!("tail-{tail}"), tail, None);
    }
    let mut before = None;
    for step in 0..40 {
        let page = page_values(&cap, &server, &base, &format!("back-{step:02}"), 2, before);
        match page["page"]["start_offset"].as_u64().unwrap() {
            0 => break,
            offset => before = Some(offset),
        }
    }
}
fn frames(cap: &Cap, label: &str, rows: &mut BufReader<TcpStream>, raw: &mut BufReader<TcpStream>) {
    let quiet = Duration::from_millis(350);
    let value = json!({"rows": collect(rows,quiet), "raw": collect(raw,quiet)});
    cap.put(
        &format!("{label}.json"),
        &serde_json::to_string_pretty(&value).unwrap(),
    );
}
fn follow() {
    let bytes = input("agy-main");
    let server = server();
    fs::write(&server.log, &bytes[..1104]).unwrap();
    let cap = Cap::new("follow-main", &server);
    let base = format!("session={}&agent=chair", server.session);
    let (mut rows, mut raw) = (open(&server, 0, true), open(&server, 0, false));
    frames(&cap, "step-00-pending", &mut rows, &mut raw);
    page_values(&cap, &server, &base, "prefix", 2000, None);
    let cut = 1168;
    server.append(std::str::from_utf8(&bytes[1104..cut]).unwrap());
    frames(&cap, "step-01-partial", &mut rows, &mut raw);
    server.append(std::str::from_utf8(&bytes[cut..1232]).unwrap());
    frames(&cap, "step-02-first-result", &mut rows, &mut raw);
    server.append(std::str::from_utf8(&bytes[1232..1358]).unwrap());
    frames(&cap, "step-03-second-result", &mut rows, &mut raw);
    page_values(&cap, &server, &base, "after-results", 2000, None);
    page_values(&cap, &server, &base, "replay", 2000, Some(1104));
    for (index, line) in bytes[1358..]
        .split_inclusive(|byte| *byte == b'\n')
        .enumerate()
    {
        server.append(std::str::from_utf8(line).unwrap());
        frames(
            &cap,
            &format!("step-04-line-{index:02}"),
            &mut rows,
            &mut raw,
        );
    }
    let page = page_values(&cap, &server, &base, "final", 2000, None);
    outputs(&cap, &server, &base, &page);
    page_values(&cap, &server, &base, "result-only", 2, Some(1358));
    frames(&cap, "step-05-quiet", &mut rows, &mut raw);
    drop(rows);
    drop(raw);
    page_values(&cap, &server, &base, "reopen", 2000, None);
}
fn overlap() {
    let bytes = input("agy-main");
    for (label, head, from) in [("window", 3476, 1104), ("pending-origin", 1104, 1104)] {
        let server = server();
        fs::write(&server.log, &bytes[..head]).unwrap();
        let cap = Cap::new(&format!("origin-{label}"), &server);
        let base = format!("session={}&agent=chair", server.session);
        let (mut rows, mut raw) = (open(&server, from, true), open(&server, from, false));
        frames(&cap, "step-00-origin", &mut rows, &mut raw);
        if head < bytes.len() {
            server.append(std::str::from_utf8(&bytes[head..]).unwrap());
            frames(&cap, "step-01-append", &mut rows, &mut raw);
        }
        let (mut rows2, mut raw2) = (open(&server, 0, true), open(&server, 0, false));
        frames(&cap, "step-02-second-client", &mut rows2, &mut raw2);
        page_values(&cap, &server, &base, "final", 2000, None);
    }
}
fn server() -> Server {
    let mut server = Server::new();
    server.log = server.dir.join(
        "home/.gemini/antigravity-cli/brain/agy-s9c/.system_generated/logs/transcript_full.jsonl",
    );
    fs::create_dir_all(server.log.parent().unwrap()).unwrap();
    fs::write(&server.log, []).unwrap();
    bind(&server, "chair", &server.log);
    server
}
fn bind(server: &Server, agent: &str, path: &Path) {
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    if agent == "chair" {
        connection
            .execute(
                "UPDATE session SET chair_provider='agy',chair_log=?1 WHERE id=?2",
                (path.to_string_lossy().as_ref(), &server.session),
            )
            .unwrap();
    } else {
        connection
            .execute(
                "UPDATE agent SET provider='agy',log=?1 WHERE session_id=?2 AND id=?3",
                (path.to_string_lossy().as_ref(), &server.session, agent),
            )
            .unwrap();
    }
}
fn bindings_and_errors() {
    for (label, agent, brain) in [
        ("chair", "chair", true),
        ("agent", "worker", true),
        ("no-brain", "chair", false),
    ] {
        let server = server();
        let path = if brain {
            server.log.clone()
        } else {
            server.dir.join("plain.jsonl")
        };
        fs::write(&path, input("agy-main")).unwrap();
        bind(&server, agent, &path);
        let cap = Cap::new(&format!("binding-{label}"), &server);
        let base = format!("session={}&agent={agent}", server.session);
        page_values(&cap, &server, &base, "full", 2000, None);
    }
    let server = server();
    fs::write(&server.log, input("agy-main")).unwrap();
    let cap = Cap::new("errors", &server);
    let base = format!("session={}&agent=chair", server.session);
    for (label, path) in [
        ("tail-0", format!("/api/rows?{base}&tail=0")),
        ("raw-tail-0", format!("/api/transcript?{base}&tail=0")),
        (
            "missing-session",
            "/api/rows?session=absent&agent=chair&tail=1".into(),
        ),
        (
            "missing-agent",
            format!("/api/rows?session={}&agent=absent&tail=1", server.session),
        ),
        (
            "output",
            format!("/api/tool-output?{base}&row=raw-1-0:output"),
        ),
    ] {
        cap.put(&format!("{label}.txt"), &get(&server, &path));
    }
    fs::remove_file(&server.log).unwrap();
    cap.put(
        "missing-log.txt",
        &get(&server, &format!("/api/rows?{base}&tail=1")),
    );
}
fn saved() {
    CAPTURE.with_borrow_mut(|values| values.clear());
    for name in [
        "agy-chair-run",
        "agy-edit",
        "agy-main",
        "agy-prefix-1104",
        "agy-prefix-1358",
        "agy-guards",
        "agy-numbers",
    ] {
        pages(name);
    }
    let server = server();
    fs::write(&server.log, input("agy-main")).unwrap();
    let cap = Cap::new("cold-result-only", &server);
    let base = format!("session={}&agent=chair", server.session);
    page_values(&cap, &server, &base, "page", 2, Some(1358));
    bindings_and_errors();
}
fn all() {
    saved();
    follow();
    overlap();
}
