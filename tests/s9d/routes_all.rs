include!("compare.rs");
include!("streams.rs");
// The same public requests capture ORIGINAL and check G2/G3; expected bytes are never generated here.
use std::{io::ErrorKind, path::Path, time::Instant};
thread_local! {
    static VALUES: std::cell::RefCell<serde_json::Map<String,Value>> = std::cell::RefCell::new(serde_json::Map::new());
}
fn input(name: &str) -> Vec<u8> {
    if std::env::var_os("S9D_CAPTURE").is_some() {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/s9d/inputs")
                .join(name),
        )
        .unwrap()
    } else {
        frozen::bytes(&format!("inputs/{name}"))
    }
}
struct Cap {
    case: String,
    session: String,
}
impl Cap {
    fn new(case: &str, server: &Server) -> Self {
        Self {
            case: format!("{}/{case}", std::env::var("S9D_MODE").unwrap()),
            session: server.session.clone(),
        }
    }
    fn put(&self, name: &str, text: &str) {
        let text = text.replace(&self.session, "SESSION");
        if let Some(root) =
            std::env::var_os("S9D_CAPTURE").or_else(|| std::env::var_os("S9D_RECORD"))
        {
            let path = Path::new(&root).join(&self.case).join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, &text).unwrap();
        }
        VALUES.with_borrow_mut(|v| {
            v.insert(format!("{}/{name}", self.case), frozen::parse(name, &text));
        });
    }
}
fn get(server: &Server, path: &str) -> String {
    let (status, _, body) = server.request("GET", path, "", "", true);
    format!("{status}\n{body}")
}
fn ids(v: &Value, found: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == "id"
                    && let Some(id) = x.as_str()
                    && (id.ends_with(":output") || id.ends_with(":diff"))
                {
                    found.push(id.to_owned());
                }
                ids(x, found);
            }
        }
        Value::Array(a) => {
            for x in a {
                ids(x, found);
            }
        }
        _ => {}
    }
}
fn outputs(server: &Server, cap: &Cap, page: &str) {
    let v: Value = serde_json::from_str(page.split_once('\n').unwrap().1).unwrap();
    let mut found = Vec::new();
    ids(&v, &mut found);
    found.sort();
    found.dedup();
    for (i, id) in found.iter().enumerate() {
        cap.put(
            &format!("output-{i:02}.txt"),
            &format!(
                "{id}\n{}",
                get(
                    server,
                    &format!(
                        "/api/tool-output?session={}&agent=chair&row={id}",
                        server.session
                    )
                )
            ),
        );
    }
}
fn setup(server: &Server) {
    let root = server.dir.join("home/.swarm");
    let c = swarm::store::open(&root.join("swarm.db")).unwrap();
    if swarm::store::orchestrator_of(&c, &server.session).is_err() {
        swarm::store::add_agent(&c, &server.session, "orchestrator", "orchestrator").unwrap();
    }
    swarm::store::set_provider(&c, &server.session, "orchestrator", "pi").unwrap();
    swarm::store::set_pane(&c, &server.session, "orchestrator", "fixture-chair").unwrap();
    swarm::store::set_provider(&c, &server.session, "worker", "pi").unwrap();
    fs::create_dir_all(root.join("adapters")).unwrap();
    fs::write(root.join("adapters/tmux-swarm.conf"),
        "self = printf fixture-chair\nlist = printf 'fixture-chair pi\\nfixture-worker pi\\n'\ncapture = cat \"$HOME/screen\"\nscreen = cat \"$HOME/screen\"\n").unwrap();
    fs::write(
        server.dir.join("home/screen"),
        include_str!("../fixtures/screens/pi-working.txt"),
    )
    .unwrap();
    fs::write(
        PathBuf::from(std::env::var_os("HOME").unwrap()).join("screen"),
        include_str!("../fixtures/screens/pi-working.txt"),
    )
    .unwrap();
}
fn bind(server: &Server, agent: &str, path: Option<&Path>, id: Option<&str>) {
    let c = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    let path = path.map(|p| p.to_string_lossy().into_owned());
    if agent == "chair" {
        c.execute("UPDATE session SET chair_provider='pi',chair_log=?1,chair_id=COALESCE(?2,chair_id) WHERE id=?3",(path,id,&server.session)).unwrap();
    } else {
        c.execute(
            "UPDATE agent SET provider='pi',log=?1 WHERE session_id=?2 AND id=?3",
            (path, &server.session, agent),
        )
        .unwrap();
    }
}
fn server(file: &str, data: &[u8]) -> Server {
    let mut s = Server::new();
    setup(&s);
    s.log = s.dir.join(file);
    fs::write(&s.log, data).unwrap();
    bind(&s, "chair", Some(&s.log), None);
    s
}
fn cases() -> Value {
    serde_json::from_str(include_str!("../fixtures/s9d/cases.json")).unwrap()
}
fn pages(case: &Value) {
    let name = case["name"].as_str().unwrap();
    let filename = case["file"].as_str().unwrap();
    let bytes = input(case["input"].as_str().unwrap_or(filename));
    let s = server(filename, &bytes);
    let cap = Cap::new(name, &s);
    let base = format!("session={}&agent=chair", s.session);
    let full = get(&s, &format!("/api/rows?{base}&tail=2000"));
    cap.put("rows-full.txt", &full);
    cap.put(
        "rows-cached.txt",
        &get(&s, &format!("/api/rows?{base}&tail=2000")),
    );
    cap.put(
        "raw-full.txt",
        &get(&s, &format!("/api/transcript?{base}&tail=2000")),
    );
    outputs(&s, &cap, &full);
    let (mut rows, mut raw) = open_pair(&s, 0, &cap, "initial-stream", Some("rows-full.txt"));
    frames(&cap, "initial-stream", &mut rows, &mut raw);
    drop(rows);
    drop(raw);
    if case["deep"] != true {
        return;
    }
    for tail in [1, 3, 8] {
        for (label, route) in [("rows", "rows"), ("raw", "transcript")] {
            cap.put(
                &format!("{label}-tail-{tail}.txt"),
                &get(&s, &format!("/api/{route}?{base}&tail={tail}")),
            );
        }
    }
    let mut before = None;
    for step in 0..40 {
        let suffix = before.map_or(String::new(), |p| format!("&before={p}"));
        let text = get(&s, &format!("/api/rows?{base}&tail=2{suffix}"));
        cap.put(&format!("rows-back-{step:02}.txt"), &text);
        cap.put(
            &format!("raw-back-{step:02}.txt"),
            &get(&s, &format!("/api/transcript?{base}&tail=2{suffix}")),
        );
        let v: Value = serde_json::from_str(text.split_once('\n').unwrap().1).unwrap();
        match v["page"]["start_offset"].as_u64() {
            Some(0) | None => break,
            Some(n) => before = Some(n),
        }
    }
    for (name, route) in [("tail", "rows"), ("raw-tail", "transcript")] {
        cap.put(
            &format!("error-{name}.txt"),
            &get(&s, &format!("/api/{route}?{base}&tail=0")),
        );
    }
    cap.put(
        "error-source.txt",
        &get(&s, "/api/rows?session=unknown&agent=chair&tail=1"),
    );
    cap.put(
        "error-output.txt",
        &get(&s, &format!("/api/tool-output?{base}&row=raw-1-0:output")),
    );
}
fn collect(stream: &mut SseStream, expected: &[Value]) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let complete = |values: &[Value]| {
        expected.iter().all(|wanted| {
            values
                .iter()
                .filter(|v| v["event"] == wanted["event"])
                .count()
                >= expected
                    .iter()
                    .filter(|v| v["event"] == wanted["event"])
                    .count()
        })
    };
    let (mut values, mut name, mut line) = (
        std::mem::take(&mut stream.pending),
        String::new(),
        String::new(),
    );
    let r = &mut stream.reader;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        r.get_ref()
            .set_read_timeout(Some(remaining.min(Duration::from_millis(900))))
            .unwrap();
        match r.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if !line.ends_with('\n') {
                    continue;
                }
                let text = std::mem::take(&mut line);
                if let Some(v) = text.strip_prefix("event: ") {
                    name = v.trim().into();
                } else if let Some(v) = text.strip_prefix("data: ")
                    && !matches!(name.as_str(), "hello" | "snapshot" | "ping")
                {
                    values.push(json!({"event":name,"data":serde_json::from_str::<Value>(v.trim()).unwrap()}));
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if complete(&values) {
                    break;
                }
            }
            Err(e) => panic!("{e}"),
        }
    }
    values
}
fn open(s: &Server, from: u64, rows: bool) -> BufReader<TcpStream> {
    let mut r = s.sse();
    let stream = named_event(&mut r, "hello")["stream"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut entry = json!({"key":format!("{}:chair",s.session),"from":from});
    if rows {
        entry["view"] = json!("rows");
    }
    let (status, _, body) = s.request(
        "POST",
        "/api/subscribe",
        "Content-Type: application/json\r\n",
        &json!({"stream":stream,"keys":[entry]}).to_string(),
        true,
    );
    assert_eq!(status, 200, "{body}");
    r
}
fn frames(cap: &Cap, name: &str, rows: &mut SseStream, raw: &mut SseStream) {
    let expected: Value =
        serde_json::from_slice(&frozen::bytes(&format!("out-1/{}/{name}.json", cap.case))).unwrap();
    let value = json!({"rows":collect(rows, expected["rows"].as_array().unwrap()),"raw":collect(raw, expected["raw"].as_array().unwrap())});
    let text = value.to_string().replace(&cap.session, "SESSION");
    let mut actual: Value = serde_json::from_str(&text).unwrap();
    for frame in actual["rows"].as_array_mut().unwrap() {
        if frame["event"] == "live" {
            frame["data"].as_object_mut().unwrap().shift_remove("live_body");
        }
    }
    rows.check.as_mut().unwrap().observe(
        name,
        actual["rows"].as_array().unwrap(),
        expected["rows"].as_array().unwrap(),
    );
    cap.put(&format!("{name}.json"), &text);
}

const MAIN: &str = "2026-11-05T00-00-00-000Z_pi-u04-main.jsonl";
fn follow() {
    let bytes = input(MAIN);
    let s = server(MAIN, &bytes[..1265]);
    let cap = Cap::new("follow", &s);
    let base = format!("session={}&agent=chair", s.session);
    let (mut rows, mut raw) = open_pair(&s, 0, &cap, "initial", None);
    frames(&cap, "initial", &mut rows, &mut raw);
    cap.put(
        "page-prefix.txt",
        &get(&s, &format!("/api/rows?{base}&tail=2000")),
    );
    cap.put(
        "page-older.txt",
        &get(&s, &format!("/api/rows?{base}&tail=2&before=1265")),
    );
    s.append(std::str::from_utf8(&bytes[1265..1400]).unwrap());
    frames(&cap, "partial", &mut rows, &mut raw);
    s.append(std::str::from_utf8(&bytes[1400..1687]).unwrap());
    frames(&cap, "results", &mut rows, &mut raw);
    cap.put(
        "page-results-window.txt",
        &get(&s, &format!("/api/rows?{base}&tail=2&before=1687")),
    );
    let (mut second_rows, mut second_raw) = open_pair(&s, 1265, &cap, "second-origin", None);
    frames(&cap, "second-origin", &mut second_rows, &mut second_raw);
    // Each complete line is a public input step; drain both readers before the next write.
    for (step, line) in std::str::from_utf8(&bytes[1687..])
        .unwrap()
        .split_inclusive('\n')
        .enumerate()
    {
        s.append(line);
        frames(&cap, &format!("rest-{step:02}"), &mut rows, &mut raw);
        frames(
            &cap,
            &format!("second-rest-{step:02}"),
            &mut second_rows,
            &mut second_raw,
        );
    }
    frames(&cap, "quiet", &mut rows, &mut raw);
    let page = get(&s, &format!("/api/rows?{base}&tail=2000"));
    cap.put("page-final.txt", &page);
    outputs(&s, &cap, &page);
    cap.put(
        "raw-final.txt",
        &get(&s, &format!("/api/transcript?{base}&tail=2000")),
    );
    cap.put(
        "page-back.txt",
        &get(&s, &format!("/api/rows?{base}&tail=2&before=1687")),
    );
    drop(rows);
    drop(raw);
    drop(second_rows);
    drop(second_raw);
    cap.put(
        "page-reopen.txt",
        &get(&s, &format!("/api/rows?{base}&tail=2000")),
    );
}
fn live() {
    let initial=b"{\"type\":\"message\",\"id\":\"live-user\",\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"show live text\"}}\n";
    let s = server("pi-live.jsonl", initial);
    let cap = Cap::new("live-file", &s);
    let file = swarm::bus::pi_live_file(&s.dir.join("home/.swarm"), &s.session, "orchestrator");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(
        &file,
        json!({"thinking":"Plan the reply","text":"The lighthouse guides boats home"}).to_string(),
    )
    .unwrap();
    let (mut rows, mut raw) = open_pair(&s, 0, &cap, "shown", None);
    frames(&cap, "shown", &mut rows, &mut raw);
    s.append("{\"type\":\"message\",\"id\":\"live-answer\",\"timestamp\":\"t\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"Plan the reply\"},{\"type\":\"text\",\"text\":\"The lighthouse guides boats home\"}],\"stopReason\":\"stop\"}}\n");
    frames(&cap, "landed", &mut rows, &mut raw);
    fs::write(file, "").unwrap();
    frames(&cap, "ended", &mut rows, &mut raw);
}
fn windows() {
    let bytes = input(MAIN);
    for (name, warm) in [("output-cold", false), ("output-retained", true)] {
        let s = server(MAIN, &bytes[..1687]);
        let cap = Cap::new(name, &s);
        let base = format!("session={}&agent=chair", s.session);
        if warm {
            cap.put(
                "rows-full.txt",
                &get(&s, &format!("/api/rows?{base}&tail=2000")),
            );
        }
        cap.put(
            "rows-window.txt",
            &get(&s, &format!("/api/rows?{base}&tail=2&before=1687")),
        );
        cap.put(
            "raw-window.txt",
            &get(&s, &format!("/api/transcript?{base}&tail=2&before=1687")),
        );
    }
}
fn origins() {
    for from in [0, 1265, 4179 + 1000] {
        let s = server(MAIN, &input(MAIN));
        let cap = Cap::new(&format!("overlap-{from}"), &s);
        let (mut rows, mut raw) = open_pair(&s, from, &cap, "initial", None);
        frames(&cap, "initial", &mut rows, &mut raw);
        s.append("{\"type\":\"message\",\"id\":\"after-overlap\",\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"after overlap\"}}\n");
        frames(&cap, "append", &mut rows, &mut raw);
        if from == 1265 {
            let (mut r2, mut n2) = open_pair(&s, 0, &cap, "recovery", None);
            frames(&cap, "recovery", &mut r2, &mut n2);
        }
    }
}
fn sources(mode: &str) {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let agent_dir = std::env::var_os("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".pi/agent"));
    for agent in ["chair", "worker"] {
        let s = server(MAIN, &input(MAIN));
        let cap = Cap::new(&format!("source-{agent}"), &s);
        let id = if agent == "chair" {
            "pi-fallback".into()
        } else {
            format!("{}-worker", s.session)
        };
        let target = if mode == "direct" && agent == "chair" {
            PathBuf::from(std::env::var_os("PI_SESSION_FILE").unwrap())
        } else {
            agent_dir
                .join("sessions/--work--")
                .join(format!("time_{id}.jsonl"))
        };
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, input(MAIN)).unwrap();
        bind(&s, agent, None, Some(&id));
        for (label, route) in [("rows", "rows"), ("raw", "transcript")] {
            cap.put(
                &format!("{label}-full.txt"),
                &get(
                    &s,
                    &format!("/api/{route}?session={}&agent={agent}&tail=2000", s.session),
                ),
            );
        }
        fs::remove_file(target).unwrap();
    }
    for agent in ["chair", "worker"] {
        let s = server(MAIN, &input(MAIN));
        let cap = Cap::new(&format!("stored-{agent}"), &s);
        bind(&s, agent, Some(&s.log), None);
        for (label, route) in [("rows", "rows"), ("raw", "transcript")] {
            cap.put(
                &format!("{label}-full.txt"),
                &get(
                    &s,
                    &format!("/api/{route}?session={}&agent={agent}&tail=2000", s.session),
                ),
            );
        }
    }
}
fn all(mode: &str) {
    if mode == "native" {
        for c in cases().as_array().unwrap() {
            pages(c);
        }
        windows();
        follow();
        origins();
        live();
    }
    sources(mode);
}
fn run_modes() {
    if let Ok(mode) = std::env::var("S9D_MODE") {
        all(&mode);
        if std::env::var_os("S9D_CAPTURE").is_none() {
            let actual = VALUES.with_borrow(|v| Value::Object(v.clone()));
            assert_capture(&mode, &actual);
        }
        return;
    }
    let root = std::env::temp_dir().join(format!("s9d-modes-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("OWNER.json"), "{\"owner\":\"orchestrator\"}\n").unwrap();
    for mode in ["native", "override", "direct"] {
        let home = root.join(mode).join("home");
        fs::create_dir_all(&home).unwrap();
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            std::thread::current().name().unwrap(),
            "--nocapture",
        ])
        .env("S9D_MODE", mode)
        .env("HOME", &home)
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("PI_SESSION_FILE");
        if mode != "native" {
            cmd.env("PI_CODING_AGENT_DIR", home.join("custom-agent"));
        }
        if mode == "direct" {
            cmd.env(
                "PI_SESSION_FILE",
                home.join("direct/time_pi-fallback.jsonl"),
            );
        }
        let output = cmd.output().unwrap();
        if !output.status.success() {
            panic!(
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}
