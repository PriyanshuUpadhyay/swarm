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
            Some("tmux-swarm"),
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
spawn = if [ -e "$HOME/fail-spawn" ]; then rm "$HOME/fail-spawn"; exit 1; fi; printf 'spawn %s %s %s\n' "$SWARM_SESSION_ID" "$SWARM_AGENT_ID" "$SWARM_ADAPTER" >> "$HOME/actions"; printf fixture-new
ring = printf 'ring %s %s %s\n' "$SWARM_SESSION_ID" "$SWARM_AGENT_ID" "$SWARM_ADAPTER" >> "$HOME/actions"; printf '%s' "$SWARM_TEXT" > "$HOME/text"
list = printf 'fixture-chair sh\nfixture-worker sh\nfixture-new sh\n'
close = printf 'close %s\n' "$SWARM_PANE" >> "$HOME/actions"
capture = cat "$HOME/screen"
screen = cat "$HOME/screen"
interrupt = printf 'interrupt %s\n' "$SWARM_PANE" >> "$HOME/actions"
key = printf 'key %s %s\n' "$SWARM_PANE" "$SWARM_KEY" >> "$HOME/actions"
"#;
            fs::write(root.join("adapters/fixture.conf"), adapter).unwrap();
            fs::write(root.join("adapters/tmux-swarm.conf"), adapter).unwrap();
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
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
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

fn named_event(reader: &mut BufReader<TcpStream>, wanted: &str) -> Value {
    loop {
        let (name, data) = event(reader);
        if name == wanted {
            return data;
        }
    }
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
    assert!(actions().contains(&format!(
        "ring {} orchestrator tmux-swarm\n",
        server.session
    )));
    assert!(actions().contains("key fixture-chair 1\n"));
    // Numbered choices send the digit alone; arrow choices send Enter (Prompt::keys).
    assert!(!actions().contains("key fixture-chair Enter\n"));
    assert!(actions().contains("interrupt fixture-chair\n"));
    let connection = rusqlite::Connection::open(server.dir.join("home/.swarm/swarm.db")).unwrap();
    // A removed worktree's chat: its folder is gone, and its pane still closes.
    connection
        .execute(
            "UPDATE session SET cwd = ?1 WHERE id = ?2",
            (
                server.dir.join("removed-worktree").to_str().unwrap(),
                &server.session,
            ),
        )
        .unwrap();
    post("/api/agent/close", target);
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
    assert!(actions().contains(&format!("spawn {session} orchestrator tmux-swarm\n")));
    post("/api/chat/new", json!({"cwd": env!("CARGO_MANIFEST_DIR")}));
}

#[path = "serve/chat.rs"]
mod chat;
#[path = "serve/choices.rs"]
mod choices;
#[path = "serve/guards.rs"]
mod guards;
#[path = "serve/handoff.rs"]
mod handoff;
#[path = "serve/transcripts.rs"]
mod transcripts;
#[path = "serve/workspace.rs"]
mod workspace;
