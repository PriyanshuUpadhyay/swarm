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
        Self::with_actions(false)
    }

    fn with_actions(actions: bool) -> Self {
        Self::configured(actions, false)
    }

    fn configured(actions: bool, lifecycle: bool) -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dir = if lifecycle {
            std::env::temp_dir()
        } else {
            repo.join("tmp/web-pilot")
        }
        .join(format!("serve-test-{}", uuid::Uuid::now_v7()));
        let home = dir.join("home");
        let root = home.join(".swarm");
        fs::create_dir_all(&root).unwrap();
        let dir = fs::canonicalize(&dir).unwrap();
        let home = dir.join("home");
        let root = home.join(".swarm");
        let source = if lifecycle {
            let source = dir.join("project");
            fs::create_dir(&source).unwrap();
            let output = Command::new("git")
                .args(["init", "-q", "-b", "main"])
                .current_dir(&source)
                .env("GIT_CEILING_DIRECTORIES", &dir)
                .output()
                .unwrap();
            assert!(output.status.success());
            source
        } else {
            repo.clone()
        };
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
            &source,
            Some(("claude", "synthetic")),
            actions.then_some("fixture"),
        )
        .unwrap();
        swarm::store::set_chair_log(&connection, &session, &log).unwrap();
        swarm::store::add_agent(&connection, &session, "worker", "chat").unwrap();
        swarm::store::set_provider(&connection, &session, "worker", "claude").unwrap();
        swarm::store::set_log(&connection, &session, "worker", &log).unwrap();
        swarm::store::add_notice(&connection, Some(&session), None, "synthetic", "notice").unwrap();
        let bin = home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        if actions {
            swarm::store::add_agent(&connection, &session, "orchestrator", "chat").unwrap();
            swarm::store::set_pane(&connection, &session, "orchestrator", "fixture-chair").unwrap();
            swarm::store::set_pane(&connection, &session, "worker", "fixture-worker").unwrap();
            fs::write(
                home.join("screen"),
                include_str!("fixtures/screens/claude-waiting.txt"),
            )
            .unwrap();
            fs::create_dir_all(root.join("adapters")).unwrap();
            let adapter = r#"self = printf fixture-chair
spawn = printf 'spawn %s %s %s\n' "$SWARM_SESSION_ID" "$SWARM_AGENT_ID" "$SWARM_ADAPTER" >> "$HOME/actions"; printf fixture-new
ring = printf 'ring %s %s %s\n' "$SWARM_SESSION_ID" "$SWARM_AGENT_ID" "$SWARM_ADAPTER" >> "$HOME/actions"; printf '%s' "$SWARM_TEXT" > "$HOME/text"
list = printf 'fixture-chair sh\nfixture-worker sh\nfixture-new sh\n'
close = printf 'close %s\n' "$SWARM_PANE" >> "$HOME/actions"
capture = cat "$HOME/screen"
screen = cat "$HOME/screen"
interrupt = printf 'interrupt %s\n' "$SWARM_PANE" >> "$HOME/actions"
key = printf 'key %s %s\n' "$SWARM_PANE" "$SWARM_KEY" >> "$HOME/actions"
"#;
            fs::write(root.join("adapters/fixture.conf"), adapter).unwrap();
            fs::write(root.join("adapters/tmux-solo.conf"), adapter).unwrap();
            fs::write(root.join("consent.json"), r#"{"trust":"standing"}"#).unwrap();
            fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
            fs::write(home.join(".config/agent-routing/roles.json"), r#"{"routes":{"chat":["fixture"]},"runners":{"fixture":{"provider":"agy","model":"flash"}}}"#).unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::write(bin.join("agy"), "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(bin.join("agy"), fs::Permissions::from_mode(0o755)).unwrap();
        }
        drop(connection);
        if lifecycle {
            // Seed the same choices as the background coordinator before checking refused requests.
            let source = source.to_string_lossy();
            fs::write(
                root.join("choices.json"),
                json!({"projectPaths":[source],"tabs":{source:{"open":[session]}}}).to_string(),
            )
            .unwrap();
        }
        let assets = dir.join("assets");
        fs::create_dir(&assets).unwrap();
        fs::write(assets.join("index.html"), "<h1>pilot</h1>").unwrap();
        fs::write(assets.join("app.js"), "console.log('pilot');").unwrap();
        std::os::unix::fs::symlink(&log, assets.join("escape.jsonl")).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &home)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env(
                "GIT_CEILING_DIRECTORIES",
                if lifecycle {
                    dir.clone()
                } else {
                    PathBuf::from("/")
                },
            )
            // Serve must remove inherited host identity before an owner key/answer call.
            .env("TMUX_PANE", "inherited-pane")
            .env("HERDR_PANE_ID", "inherited-pane")
            .env("SWARM_SESSION_ID", "inherited-session")
            .env("SWARM_AGENT_ID", "inherited-worker")
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
        let Some((address, token)) = first.trim().split_once("/#token=") else {
            let _ = child.kill();
            let _ = child.wait();
            fs::remove_dir_all(&dir).unwrap();
            panic!("serve failed before the server URL and token");
        };
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
        if path == "/api/chat/continue" {
            stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        }
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

#[test]
fn handoff_http_progress_delivery_link_and_cancel() {
    use std::os::unix::fs::PermissionsExt;
    for cancel in [false, true] {
        let server = Server::with_actions(true);
        let home = server.dir.join("home");
        let root = home.join(".swarm");
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join("bin/claude"), "#!/bin/sh\nif [ \"$*\" = 'auth status' ]; then printf '%s\\n' '{\"loggedIn\":true}'; fi\n").unwrap();
        fs::set_permissions(home.join("bin/claude"), fs::Permissions::from_mode(0o755)).unwrap();
        let summary = [
            json!({"type":"user","uuid":"summary-request","message":{"role":"user","content":swarm::view::handoff::REQUEST}}),
            json!({"type":"assistant","uuid":"summary-answer","message":{"role":"assistant","content":[{"type":"text","text":"Keep the files and check the menu"}]}}),
            json!({"type":"system","subtype":"turn_duration","uuid":"summary-end","durationMs":1}),
        ].iter().map(|e| format!("{e}\n")).collect::<String>();
        fs::write(home.join("summary-turn.jsonl"), summary).unwrap();
        fs::write(
            home.join("handoff-ring"),
            format!(
                r#"#!/bin/sh
printf 'ring %s\n' "$SWARM_PANE" >> "$HOME/actions"
printf '%s' "$SWARM_TEXT" > "$HOME/text"
if [ "$SWARM_PANE" = fixture-chair ] && [ ! -f "$HOME/no-summary" ]; then
  cat "$HOME/summary-turn.jsonl" >> '{}'
fi
"#,
                server.log.display().to_string().replace('\'', "'\"'\"'")
            ),
        )
        .unwrap();
        fs::set_permissions(home.join("handoff-ring"), fs::Permissions::from_mode(0o755)).unwrap();
        for adapter in ["fixture", "tmux-solo"] {
            let file = root.join(format!("adapters/{adapter}.conf"));
            let text = fs::read_to_string(&file).unwrap();
            let text = text
                .lines()
                .map(|line| {
                    if line.starts_with("ring =") {
                        "ring = \"$HOME/handoff-ring\""
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(file, format!("{text}\n")).unwrap();
        }
        if cancel {
            fs::write(home.join("no-summary"), "").unwrap();
        }
        let mut stream = server.sse();
        stream.get_ref().set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        assert_eq!(event(&mut stream).0, "hello");
        std::thread::scope(|scope| {
            let server_ref = &server;
            let request=scope.spawn(move || server_ref.request("POST","/api/chat/continue","Content-Type: application/json\r\n",&json!({"old_session":server_ref.session,"provider":"claude","model":"sonnet","account":"default"}).to_string(),true));
            let mut phases = Vec::new();
            loop {
                let progress = named_event(&mut stream, "handoff");
                assert_eq!(progress["old_session"], server.session);
                let phase = progress["phase"].as_str().unwrap().to_owned();
                phases.push(phase.clone());
                if cancel && phase == "summarizing" {
                    let (status, _, body) = server.request(
                        "POST",
                        "/api/chat/continue/cancel",
                        "Content-Type: application/json\r\n",
                        &json!({"old_session":server.session}).to_string(),
                        true,
                    );
                    assert_eq!(status, 200, "{body}");
                }
                if !cancel && phase == "waiting" {
                    assert_eq!(progress["canCancel"], false);
                    let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
                    let new_id: String = connection
                        .query_row(
                            "SELECT id FROM session WHERE id != ?1 ORDER BY id DESC LIMIT 1",
                            [&server.session],
                            |r| r.get(0),
                        )
                        .unwrap();
                    swarm::store::set_chair(
                        &connection,
                        &new_id,
                        Some(("claude", "stand-in-ready")),
                    )
                    .unwrap();
                }
                if matches!(phase.as_str(), "completed" | "cancelled" | "failed") {
                    break;
                }
            }
            let (status, _, body) = request.join().unwrap();
            let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
            if cancel {
                assert_eq!(status, 409, "{body}");
                assert_eq!(phases, ["preparing", "summarizing", "cancelled"]);
                let count: i64 = connection
                    .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(count, 1);
                let actions = fs::read_to_string(home.join("actions")).unwrap_or_default();
                assert!(!actions.contains("spawn"));
                assert!(!actions.contains("interrupt"));
            } else {
                assert_eq!(status, 200, "{body}");
                assert_eq!(
                    phases,
                    [
                        "preparing",
                        "summarizing",
                        "starting",
                        "waiting",
                        "delivering",
                        "completed"
                    ]
                );
                let response: Value = serde_json::from_str(&body).unwrap();
                let linked: String = connection
                    .query_row(
                        "SELECT continuation_of FROM session WHERE id = ?1",
                        [response["session"].as_str().unwrap()],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(linked, server.session);
                let delivered = fs::read_to_string(home.join("text")).unwrap();
                assert!(delivered.contains("compact handoff summary:"));
                assert!(delivered.contains("Keep the files and check the menu"));
            }
        });
    }
}

#[test]
fn menu_http_matches_cli_json_and_keeps_bearer_guards() {
    let server = Server::with_actions(true);
    let home = server.dir.join("home");
    for (route, args) in [
        ("/api/roles", vec!["roles", "--json"]),
        ("/api/providers", vec!["providers", "--json"]),
        (
            "/api/models?provider=claude",
            vec!["models", "--provider", "claude", "--json"],
        ),
        (
            "/api/accounts?provider=agy",
            vec!["accounts", "--provider", "agy", "--json"],
        ),
    ] {
        let (status, _, body) = server.request("GET", route, "", "", true);
        assert_eq!(status, 200, "{body}");
        let cli = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &home)
            .env("SWARM_HOME", &home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .args(args)
            .output()
            .unwrap();
        assert!(
            cli.status.success(),
            "{}",
            String::from_utf8_lossy(&cli.stderr)
        );
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            serde_json::from_slice::<Value>(&cli.stdout).unwrap()
        );
        assert_eq!(server.request("GET", route, "", "", false).0, 401);
    }
    for route in ["/api/models", "/api/accounts?provider=unknown"] {
        assert_eq!(server.request("GET", route, "", "", true).0, 400);
    }
    for route in ["/api/chat/continue", "/api/chat/continue/cancel"] {
        assert_eq!(
            server
                .request(
                    "POST",
                    route,
                    "Content-Type: application/json\r\n",
                    "{}",
                    false
                )
                .0,
            401
        );
        assert_eq!(
            server
                .request(
                    "POST",
                    route,
                    "Content-Type: application/json\r\n",
                    "{}",
                    true
                )
                .0,
            400
        );
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

#[test]
fn chat_posts_are_guarded_before_any_action_runs() {
    let server = Server::with_actions(true);
    let body =
        json!({"session": server.session, "agent": "chair", "text": "must not run"}).to_string();
    for (headers, auth, expected) in [
        ("Content-Type: application/json\r\n", false, 401),
        (
            "Content-Type: application/json\r\nOrigin: https://evil.test\r\n",
            true,
            403,
        ),
        ("Content-Type: text/plain\r\n", true, 415),
    ] {
        assert_eq!(
            server
                .request("POST", "/api/chat/send", headers, &body, auth)
                .0,
            expected
        );
    }
    let oversized =
        json!({"session":server.session,"agent":"chair","text":"x".repeat(65536)}).to_string();
    assert_eq!(
        server
            .request(
                "POST",
                "/api/chat/send",
                "Content-Type: application/json\r\n",
                &oversized,
                true
            )
            .0,
        413
    );
    assert!(!server.dir.join("home/actions").exists());
}

#[test]
fn chat_routes_run_the_real_cli_against_a_fake_provider_and_adapter() {
    let server = Server::with_actions(true);
    let post = |route: &str, body: Value| {
        let (status, _, response) = server.request(
            "POST",
            route,
            "Content-Type: application/json\r\n",
            &body.to_string(),
            true,
        );
        assert_eq!(status, 200, "{route}: {response}");
        let response: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["ok"], true);
        response
    };
    let target = json!({"session": server.session, "agent": "chair"});
    let text = "quotes ' and $(touch injected)\nsecond line";
    let mut send = target.clone();
    send["text"] = json!(text);
    let mut stream = server.sse();
    let _ = named_event(&mut stream, "hello");
    post("/api/chat/send", send);
    assert!(
        named_event(&mut stream, "snapshot")["version"]
            .as_u64()
            .unwrap()
            > 1
    );
    assert_eq!(
        fs::read_to_string(server.dir.join("home/text")).unwrap(),
        text
    );
    assert!(!server.dir.join("home/injected").exists());
    post("/api/chat/interrupt", target.clone());
    for key in ["Up", "C-u"] {
        let mut body = target.clone();
        body["key"] = json!(key);
        post("/api/chat/key", body);
    }
    let prompt =
        swarm::screen::prompt(include_str!("fixtures/screens/claude-waiting.txt")).unwrap();
    let mut answer = target.clone();
    answer["prompt_id"] = json!(prompt.id);
    answer["choice"] = json!(0);
    post("/api/chat/answer", answer);
    let actions = || fs::read_to_string(server.dir.join("home/actions")).unwrap();
    assert!(actions().contains(&format!("ring {} orchestrator fixture\n", server.session)));
    assert!(actions().contains("key fixture-chair 1\n"));
    // Numbered choices send the digit alone; arrow choices send Enter (Prompt::keys).
    assert!(!actions().contains("key fixture-chair Enter\n"));
    assert!(actions().contains("interrupt fixture-chair\n"));
    post("/api/agent/close", target);
    let connection = rusqlite::Connection::open(server.dir.join("home/.swarm/swarm.db")).unwrap();
    assert_eq!(
        swarm::store::pane_of(&connection, &server.session, "orchestrator").unwrap(),
        None
    );
    assert!(actions().contains("close fixture-chair\n"));
    post("/api/sessions/archive", json!({"ids": [server.session]}));
    let (status, _, body) = server.request("GET", "/api/sessions/archived", "", "", true);
    assert_eq!(status, 200);
    let archived: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(archived["sessions"][0]["id"], server.session);
    assert!(archived["sessions"][0]["archivedAt"].as_i64().is_some());
    post("/api/sessions/unarchive", json!({"ids": [server.session]}));
    let (_, _, body) = server.request("GET", "/api/sessions/archived", "", "", true);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["sessions"],
        json!([])
    );
    let created = post(
        "/api/chat/new",
        json!({"cwd": env!("CARGO_MANIFEST_DIR"), "provider":"agy", "model":"flash", "account":"auto"}),
    );
    let session = created["session"].as_str().unwrap();
    assert_ne!(session, server.session);
    assert_eq!(
        swarm::store::session_cwd(&connection, session)
            .unwrap()
            .as_deref(),
        Some(env!("CARGO_MANIFEST_DIR"))
    );
    assert_eq!(
        swarm::store::pane_of(&connection, session, "orchestrator")
            .unwrap()
            .as_deref(),
        Some("fixture-new")
    );
    assert!(actions().contains(&format!("spawn {session} orchestrator tmux-solo\n")));
    post("/api/chat/new", json!({"cwd": env!("CARGO_MANIFEST_DIR")}));
}

fn tree_choices(server: &Server) -> Value {
    let (status, _, body) = server.request("GET", "/api/tree?view=workspaces", "", "", true);
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

#[test]
fn workspace_lifecycle_http_refuses_unlisted_paths_before_any_change() {
    let server = Server::configured(true, true);
    let outside = server.dir.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep").unwrap();
    let root = server.dir.join("home/.swarm");
    let before = fs::read(root.join("choices.json")).ok();
    for (route, key) in [("new-options", "project"), ("impact", "workspace")] {
        assert_eq!(
            server
                .request(
                    "GET",
                    &format!("/api/workspace/{route}?{key}={}", outside.display()),
                    "",
                    "",
                    true
                )
                .0,
            403
        );
    }
    for route in ["new", "archive", "restore", "delete", "prune"] {
        let body = if route == "new" {
            json!({"project":outside,"name":"Escape","mode":"new-branch"})
        } else {
            json!({"workspace":outside})
        };
        let (status, _, body) = server.request(
            "POST",
            &format!("/api/workspace/{route}"),
            "Content-Type: application/json\r\n",
            &body.to_string(),
            true,
        );
        assert_eq!(status, 403, "{route}: {body}");
    }
    assert_eq!(fs::read(root.join("choices.json")).ok(), before);
    assert_eq!(
        fs::read_to_string(outside.join("keep.txt")).unwrap(),
        "keep"
    );
    assert!(!server.dir.join("home/actions").exists());
}

#[test]
fn project_remove_http_refuses_unlisted_path_and_keeps_folder() {
    let server = Server::configured(true, true);
    let outside = server.dir.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep").unwrap();
    let root = server.dir.join("home/.swarm");
    let before = fs::read(root.join("choices.json")).ok();
    let (status, _, body) = server.request(
        "POST",
        "/api/project/remove",
        "Content-Type: application/json\r\n",
        &json!({"project":outside}).to_string(),
        true,
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(fs::read(root.join("choices.json")).ok(), before);
    assert_eq!(
        fs::read_to_string(outside.join("keep.txt")).unwrap(),
        "keep"
    );
    assert!(!server.dir.join("home/actions").exists());
}

#[test]
fn project_create_import_http_refuse_swarm_home_symlink_and_keep_data() {
    let server = Server::configured(true, true);
    let root = server.dir.join("home/.swarm");
    let link = server.dir.join("home-link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    fs::write(root.join("keep.txt"), "keep").unwrap();
    let before = fs::read(root.join("choices.json")).ok();
    for (route, value) in [
        (
            "create",
            json!({"path":link.join("escape"),"name":"Escape"}),
        ),
        ("import", json!({"path":link,"as":"git"})),
    ] {
        let path = format!("/api/project/{route}");
        assert_eq!(
            server
                .request(
                    "POST",
                    &path,
                    "Content-Type: application/json\r\n",
                    &value.to_string(),
                    false
                )
                .0,
            401
        );
        assert_eq!(
            server
                .request(
                    "POST",
                    &path,
                    "Content-Type: application/json\r\nOrigin: https://evil.test\r\n",
                    &value.to_string(),
                    true
                )
                .0,
            403
        );
        let (status, _, body) = server.request(
            "POST",
            &path,
            "Content-Type: application/json\r\n",
            &value.to_string(),
            true,
        );
        assert_eq!(status, 403, "{body}");
    }
    assert_eq!(fs::read(root.join("choices.json")).ok(), before);
    assert_eq!(fs::read_to_string(root.join("keep.txt")).unwrap(), "keep");
    assert!(!root.join("escape").exists());
    assert!(!root.join(".git").exists());
    assert!(!server.dir.join("home/actions").exists());
}
fn write_choices(server: &Server, mut operation: Value) -> Value {
    for _ in 0..5 {
        operation["revision"] = tree_choices(server)["revision"].clone();
        let (status, _, body) = server.request(
            "POST",
            "/api/choices",
            "Content-Type: application/json\r\n",
            &operation.to_string(),
            true,
        );
        if status == 409 {
            continue;
        }
        assert_eq!(status, 200, "{body}");
        return serde_json::from_str(&body).unwrap();
    }
    panic!("choices did not settle after discovery");
}
#[test]
fn stale_choices_http_changes_nothing_and_routes_are_guarded() {
    let server = Server::new();
    let workspace = env!("CARGO_MANIFEST_DIR");
    assert_eq!(server.request("GET", "/api/tree", "", "", false).0, 401);
    assert_eq!(
        server
            .request("GET", "/api/tabs?workspace=x", "", "", false)
            .0,
        401
    );
    assert_eq!(
        server
            .request(
                "POST",
                "/api/choices",
                "Content-Type: application/json\r\n",
                "{}",
                false
            )
            .0,
        401
    );
    assert_eq!(
        server
            .request("GET", "/api/tree?view=other", "", "", true)
            .0,
        400
    );
    let old = tree_choices(&server)["revision"].clone();
    write_choices(
        &server,
        json!({"op":"rename_workspace","workspace":workspace,"name":"Saved"}),
    );
    let root = server.dir.join("home/.swarm");
    let choices = fs::read(root.join("choices.json")).unwrap();
    let state = fs::read(root.join("view-state.json")).ok();
    let (status, _, body) = server.request(
        "POST",
        "/api/choices",
        "Content-Type: application/json\r\n",
        &json!({"revision":old,"op":"rename_workspace","workspace":workspace,"name":"Stale"})
            .to_string(),
        true,
    );
    assert_eq!(status, 409, "{body}");
    assert_eq!(fs::read(root.join("choices.json")).unwrap(), choices);
    assert_eq!(fs::read(root.join("view-state.json")).ok(), state);
    assert_eq!(
        server
            .request("GET", "/api/tabs?workspace=%2Fmissing", "", "", true)
            .0,
        404
    );
    assert_eq!(
        server
            .request("GET", "/api/tree?view=archived", "", "", true)
            .0,
        200
    );
}
#[cfg(target_os = "macos")]
#[test]
fn web_choices_are_read_by_production_swift_decoder_fixture() {
    let server = Server::new();
    let workspace = env!("CARGO_MANIFEST_DIR");
    write_choices(&server, json!({"op":"pin","workspace":workspace}));
    write_choices(
        &server,
        json!({"op":"rename_workspace","workspace":workspace,"name":"From web"}),
    );
    let source = include_str!("../ui/Sources/SwarmCore/Agent/OwnerChoices.swift")
        .split("public struct OwnerChoicesFailure")
        .next()
        .unwrap();
    fs::write(server.dir.join("OwnerChoices.swift"), source).unwrap();
    let fields = include_str!("../ui/Sources/SwarmCore/Presentation/RowFields.swift")
        .split("public struct RowFieldValue")
        .next()
        .unwrap();
    fs::write(server.dir.join("RowFields.swift"), fields).unwrap();
    fs::write(
        server.dir.join("TabStrip.swift"),
        include_str!("../ui/Sources/SwarmCore/Presentation/TabStrip.swift"),
    )
    .unwrap();
    fs::write(
        server.dir.join("main.swift"),
        include_str!("fixtures/choices-parser/main.swift"),
    )
    .unwrap();
    let output = Command::new("/usr/bin/swiftc")
        .arg("-module-cache-path")
        .arg(server.dir.join("swift-cache"))
        .args([
            server.dir.join("OwnerChoices.swift"),
            server.dir.join("RowFields.swift"),
            server.dir.join("TabStrip.swift"),
            server.dir.join("main.swift"),
        ])
        .arg("-o")
        .arg(server.dir.join("choices-parser"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(server.dir.join("choices-parser"))
        .arg(server.dir.join("home/.swarm/choices.json"))
        .arg(workspace)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("PASS"));
}
#[test]
fn choices_external_refresh_busy_and_corrupt_state_keep_tree_visible() {
    let server = Server::new();
    let workspace = env!("CARGO_MANIFEST_DIR");
    let first = tree_choices(&server);
    let root = server.dir.join("home/.swarm");
    let mut owner: Value = fs::read(root.join("choices.json"))
        .ok()
        .map(|b| serde_json::from_slice(&b).unwrap())
        .unwrap_or(json!({}));
    owner["names"] = json!({workspace:"External name"});
    fs::write(root.join("choices.json"), owner.to_string()).unwrap();
    let second = tree_choices(&server);
    assert_ne!(first["revision"], second["revision"]);
    assert!(second.to_string().contains("External name"));
    let held = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("choices.lock"))
        .unwrap();
    held.lock().unwrap();
    let busy = tree_choices(&server);
    assert!(busy["choicesError"].as_str().is_some());
    assert!(busy.to_string().contains("External name"));
    drop(held);
    fs::write(root.join("view-state.json"), "{broken").unwrap();
    let failed = tree_choices(&server);
    assert!(failed["choicesError"].as_str().is_some());
    assert!(failed.to_string().contains("External name"));
}
