#![cfg(feature = "serve")]

use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Server {
    child: Child,
    dir: PathBuf,
    log: PathBuf,
    session: String,
    port: u16,
    token: String,
}

impl Server {
    fn new() -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dir = repo
            .join("tmp/web-pilot")
            .join(format!("serve-test-{}", uuid::Uuid::now_v7()));
        let home = dir.join("home");
        let root = home.join(".swarm");
        fs::create_dir_all(&root).unwrap();
        let log = dir.join("chair.jsonl");
        fs::copy(
            repo.join("packages/transcript/src/fixtures/claude-chair-run.jsonl"),
            &log,
        )
        .unwrap();
        let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            &repo,
            Some(("claude", "synthetic")),
            None,
        )
        .unwrap();
        swarm::store::set_chair_log(&connection, &session, &log).unwrap();
        swarm::store::add_agent(&connection, &session, "worker", "chat").unwrap();
        swarm::store::set_provider(&connection, &session, "worker", "claude").unwrap();
        swarm::store::set_log(&connection, &session, "worker", &log).unwrap();
        swarm::store::add_notice(&connection, Some(&session), None, "synthetic", "notice").unwrap();
        drop(connection);
        let assets = dir.join("assets");
        fs::create_dir(&assets).unwrap();
        fs::write(assets.join("index.html"), "<h1>pilot</h1>").unwrap();
        fs::write(assets.join("app.js"), "console.log('pilot');").unwrap();
        std::os::unix::fs::symlink(&log, assets.join("escape.jsonl")).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .args(["serve", "--port", "0", "--assets"])
            .arg(assets)
            .args(["--dev-origin", "http://127.0.0.1:5173"])
            .env("SWARM_HOME", home)
            .env(
                "SWARM_TRANSCRIPT_TOOL",
                repo.join("packages/transcript/zig-out/bin/transcript"),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut first = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut first)
            .unwrap();
        let (address, token) = first
            .trim()
            .split_once("/#token=")
            .expect("server URL and token");
        let port = address.rsplit_once(':').unwrap().1.parse().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        Self {
            child,
            dir,
            log,
            session,
            port,
            token: token.to_owned(),
        }
    }

    fn connection(&self) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        headers: &str,
        body: &str,
        auth: bool,
    ) -> (u16, String, String) {
        let mut stream = self.connection();
        let token = if auth {
            format!("Authorization: Bearer {}\r\n", self.token)
        } else {
            String::new()
        };
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{token}{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", self.port, body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, headers.to_owned(), body.to_owned())
    }

    fn sse(&self) -> BufReader<TcpStream> {
        let mut stream = self.connection();
        write!(
            stream,
            "GET /api/stream HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n\r\n",
            self.port, self.token
        )
        .unwrap();
        let mut reader = BufReader::new(stream);
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        assert!(first.contains("200"), "{first}");
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        reader
    }

    fn subscribe(&self, stream: &str, from: u64) {
        let body = json!({"stream": stream, "keys": [{"key": format!("{}:chair", self.session), "from": from}]}).to_string();
        let (status, _, body) = self.request(
            "POST",
            "/api/subscribe",
            "Content-Type: application/json\r\n",
            &body,
            true,
        );
        assert_eq!(status, 200, "{body}");
    }

    fn append(&self, bytes: &str) {
        OpenOptions::new()
            .append(true)
            .open(&self.log)
            .unwrap()
            .write_all(bytes.as_bytes())
            .unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        fs::remove_dir_all(&self.dir).unwrap();
    }
}

fn event(reader: &mut BufReader<TcpStream>) -> (String, Value) {
    let mut name = String::new();
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0, "SSE ended");
        if let Some(value) = line.strip_prefix("event: ") {
            name = value.trim().to_owned();
        }
        if let Some(value) = line.strip_prefix("data: ") {
            return (name, serde_json::from_str(value.trim()).unwrap());
        }
    }
}

fn named_event(reader: &mut BufReader<TcpStream>, wanted: &str) -> Value {
    loop {
        let (name, data) = event(reader);
        if name == wanted {
            return data;
        }
    }
}

#[test]
fn bearer_host_origin_and_body_guards_run_on_the_real_server() {
    let server = Server::new();
    let (status, headers, _) = server.request("GET", "/api/snapshot", "", "", false);
    assert_eq!(status, 401);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("www-authenticate: bearer realm=\"swarm\"")
    );
    assert!(!headers.contains("invalid_token"));
    let (status, headers, _) = server.request(
        "GET",
        "/api/snapshot",
        "Authorization: Bearer wrong\r\n",
        "",
        false,
    );
    assert_eq!(status, 401);
    assert!(headers.contains("error=\"invalid_token\""));
    let mut stream = server.connection();
    write!(stream, "GET /api/snapshot HTTP/1.1\r\nHost: evil.test\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n", server.token).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    assert_eq!(
        server
            .request(
                "GET",
                "/api/snapshot",
                "Origin: https://evil.test\r\n",
                "",
                true
            )
            .0,
        403
    );
    assert_eq!(
        server.request("POST", "/api/subscribe", "", "{}", true).0,
        415
    );
    let (status, headers, _) = server.request(
        "GET",
        "/api/snapshot",
        "Origin: http://127.0.0.1:5173\r\n",
        "",
        true,
    );
    assert_eq!(status, 200);
    assert!(!headers.to_ascii_lowercase().contains("access-control-"));
    let body = json!({"stream": "unknown", "keys": [], "padding": "x".repeat(65536)}).to_string();
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
        413
    );
    assert_eq!(server.request("GET", "/api/unknown", "", "", false).0, 401);
}

#[test]
fn the_coordinator_leaves_notices_for_the_native_shell() {
    let server = Server::new();
    // Two coordinator passes run in this wait.
    std::thread::sleep(Duration::from_millis(2200));
    let connection = rusqlite::Connection::open(server.dir.join("home/.swarm/swarm.db")).unwrap();
    let unposted: i64 = connection
        .query_row(
            "SELECT count(*) FROM notice WHERE posted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unposted, 1);
}

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
