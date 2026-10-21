//! A terminal per agent and per background shell, over one WebSocket (ADR 0102).
//!
//! A WebSocket cannot carry the bearer header, so the page first asks `/api/terminal/ticket`
//! (bearer-checked) for a ticket. The ticket is random, single use, short-lived, and names its one
//! target, and `/api/terminal?ticket=` reads no other target. An agent's terminal runs the
//! adapter's `attach` in a PTY; a shell's terminal only streams its output file.

use super::*;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

pub(super) const SOCKET_PATH: &str = "/api/terminal";
const TICKET_TTL: Duration = Duration::from_secs(30);
const MAX_TICKETS: usize = 16;
const MAX_LIVE: usize = 8;
/// How much of a shell's output file a new terminal shows first.
const FILE_TAIL: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Target {
    Attach { adapter: String, pane: String },
    File(PathBuf),
}

static TICKETS: Mutex<Vec<(String, Instant, Target)>> = Mutex::new(Vec::new());
static LIVE: AtomicUsize = AtomicUsize::new(0);

fn mint(target: Target) -> Result<String, ApiError> {
    let mut tickets = TICKETS.lock().unwrap();
    tickets.retain(|(_, made, _)| made.elapsed() < TICKET_TTL);
    if tickets.len() >= MAX_TICKETS {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "too many terminal tickets",
        ));
    }
    let ticket =
        new_token().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "ticket failed"))?;
    tickets.push((ticket.clone(), Instant::now(), target));
    Ok(ticket)
}

/// Takes the ticket out, so a second use finds nothing.
fn redeem(ticket: &str) -> Option<Target> {
    let mut tickets = TICKETS.lock().unwrap();
    let at = tickets.iter().position(|(t, _, _)| t == ticket)?;
    let (_, made, target) = tickets.swap_remove(at);
    (made.elapsed() < TICKET_TTL).then_some(target)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TicketRequest {
    session: String,
    agent: String,
    task: Option<String>,
}

pub(super) async fn terminal_ticket(
    State(app): State<App>,
    Json(request): Json<TicketRequest>,
) -> Result<Json<Value>, ApiError> {
    let target = tokio::task::spawn_blocking(move || target(&app, &request))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "ticket failed"))??;
    Ok(Json(json!({"ticket": mint(target)?})))
}

fn target(app: &Server, request: &TicketRequest) -> Result<Target, ApiError> {
    if let Some(task) = &request.task {
        let path = session_tasks(app, &request.session)
            .into_iter()
            .find(|t| t.id == *task && t.agent == request.agent && t.kind == "shell")
            .and_then(|t| t.output)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown background shell"))?;
        return output_file(&app.root, &path)
            .map(Target::File)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "shell output unavailable"));
    }
    let pane = {
        let hub = app.hub.lock().unwrap();
        hub.agents[&request.session]
            .as_array()
            .into_iter()
            .flatten()
            .find(|agent| agent["id"] == request.agent.as_str() && agent["alive"] == true)
            .and_then(|agent| agent["pane"].as_str().map(str::to_string))
    }
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "no live agent"))?;
    let connection = read_connection(&app.root)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?;
    let adapter = swarm::store::adapter_of(&connection, &request.session)
        .ok()
        .flatten()
        .unwrap_or_default();
    // Only this adapter is known to run each agent in a session of its own, so an attach shows
    // just that agent and restores its size (adapters/tmux-swarm.conf).
    if adapter != "tmux-swarm" {
        return Err(error(
            StatusCode::CONFLICT,
            "this chat's panes have no terminal here",
        ));
    }
    Ok(Target::Attach { adapter, pane })
}

/// The canonical path of a Claude task output file this user owns, or None. Claude Code writes
/// them under `/tmp/claude-<uid>/`; the log names the path, so the path is checked, not trusted.
pub(super) fn output_file(root: &Path, path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata(root).ok()?.uid();
    let base = Path::new("/tmp")
        .join(format!("claude-{uid}"))
        .canonicalize()
        .ok()?;
    let path = path.canonicalize().ok()?;
    let meta = std::fs::metadata(&path).ok()?;
    (path.starts_with(&base)
        && path.extension().is_some_and(|ext| ext == "output")
        && meta.is_file()
        && meta.uid() == uid)
        .then_some(path)
}

#[derive(Deserialize)]
pub(super) struct SocketQuery {
    ticket: String,
}

pub(super) async fn terminal_socket(
    State(app): State<App>,
    Query(query): Query<SocketQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(target) = redeem(&query.ticket) else {
        return error(StatusCode::UNAUTHORIZED, "terminal ticket required").into_response();
    };
    if LIVE.fetch_add(1, Ordering::SeqCst) >= MAX_LIVE {
        LIVE.fetch_sub(1, Ordering::SeqCst);
        return error(StatusCode::TOO_MANY_REQUESTS, "too many terminals").into_response();
    }
    upgrade.on_upgrade(move |socket| async move {
        match target {
            Target::Attach { adapter, pane } => attach(&app.root, &adapter, &pane, socket).await,
            Target::File(path) => follow_file(path, socket).await,
        }
        LIVE.fetch_sub(1, Ordering::SeqCst);
    })
}

/// The adapter's `attach` for `pane`, ready for a PTY: a terminal type xterm.js draws, UTF-8, and
/// no `TMUX`, since a server started from tmux would make tmux refuse to nest.
pub(super) fn attach_command(
    root: &Path,
    adapter: &str,
    pane: &str,
) -> Result<CommandBuilder, Box<dyn std::error::Error>> {
    let command = swarm::adapter::load(root, adapter)?.attach_command(&[("pane", pane)])?;
    let mut builder = CommandBuilder::new(command.get_program());
    builder.args(command.get_args());
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => builder.env(key, value),
            None => builder.env_remove(key),
        }
    }
    builder.env_remove("TMUX");
    builder.env_remove("TMUX_PANE");
    builder.env("TERM", "xterm-256color");
    builder.env("LANG", "en_US.UTF-8");
    Ok(builder)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Control {
    Resize { cols: u16, rows: u16 },
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols: cols.clamp(2, 1000),
        rows: rows.clamp(2, 500),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Bytes both ways between the socket and a PTY that runs the adapter's `attach`. Leaving closes
/// the PTY, which ends only the tmux client; the agent's session stays.
async fn attach(root: &Path, adapter: &str, pane: &str, mut socket: WebSocket) {
    let started = attach_command(root, adapter, pane)
        .and_then(|command| {
            let pair = native_pty_system().openpty(size(80, 24))?;
            let child = pair.slave.spawn_command(command)?;
            drop(pair.slave);
            let reader = pair.master.try_clone_reader()?;
            let writer = pair.master.take_writer()?;
            Ok((pair.master, child, reader, writer))
        })
        .map_err(|err| err.to_string());
    let (master, mut child, mut reader, mut writer) = match started {
        Ok(parts) => parts,
        Err(err) => {
            let _ = socket
                .send(Message::Text(format!("swarm: {err}\r\n").into()))
                .await;
            return;
        }
    };
    let (output, mut from_pty) = mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || output.blocking_send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let (input, to_pty) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        for bytes in to_pty {
            if writer
                .write_all(&bytes)
                .and_then(|()| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });
    loop {
        tokio::select! {
            bytes = from_pty.recv() => match bytes {
                Some(bytes) => {
                    if socket.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            message = socket.recv() => match message {
                Some(Ok(Message::Binary(bytes))) => {
                    if input.send(bytes.to_vec()).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    if let Ok(Control::Resize { cols, rows }) = serde_json::from_str(&text) {
                        let _ = master.resize(size(cols, rows));
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    drop(input);
    drop(master);
    let _ = socket.send(Message::Close(None)).await;
    // The closed PTY ends the client and the verb restores the window size; a child that is
    // still there after that is killed, and every child is reaped.
    let _ = tokio::task::spawn_blocking(move || {
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
    })
    .await;
}

/// A read-only view of a shell's output file: its last part, then whatever it grows by. Input from
/// the socket is dropped, as nothing outside Claude Code can type into its shell.
async fn follow_file(path: PathBuf, mut socket: WebSocket) {
    use std::io::{Seek, SeekFrom};
    let mut at = std::fs::metadata(&path)
        .map(|meta| meta.len().saturating_sub(FILE_TAIL))
        .unwrap_or(0);
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let len = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                if len < at {
                    at = 0;
                }
                if len == at {
                    continue;
                }
                let read = std::fs::File::open(&path).and_then(|mut file| {
                    file.seek(SeekFrom::Start(at))?;
                    let mut bytes = Vec::new();
                    file.take(FILE_TAIL).read_to_end(&mut bytes)?;
                    Ok(bytes)
                });
                let Ok(bytes) = read else { continue };
                at += bytes.len() as u64;
                if socket.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            message = socket.recv() => match message {
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_works_once_and_names_its_target() {
        let target = Target::File(PathBuf::from("/tmp/x.output"));
        let ticket = mint(target.clone()).unwrap();
        assert_eq!(ticket.len(), 64);
        assert_eq!(redeem(&ticket), Some(target));
        assert_eq!(redeem(&ticket), None);
        assert_eq!(redeem("guess"), None);
    }

    #[test]
    fn an_output_file_must_be_a_claude_task_file_of_this_user() {
        use std::os::unix::fs::MetadataExt;
        let root = std::env::temp_dir();
        let uid = std::fs::metadata(&root).unwrap().uid();
        let dir = PathBuf::from(format!(
            "/tmp/claude-{uid}/swarm-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("b1.output");
        std::fs::write(&good, "x").unwrap();
        let other = dir.join("notes.txt");
        std::fs::write(&other, "x").unwrap();
        let link = dir.join("link.output");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink("/etc/hosts", &link).unwrap();

        assert_eq!(
            output_file(&root, &good),
            Some(good.canonicalize().unwrap())
        );
        assert_eq!(output_file(&root, &other), None);
        assert_eq!(output_file(&root, &link), None);
        assert_eq!(output_file(&root, &dir.join("../../etc/hosts")), None);
        assert_eq!(output_file(&root, Path::new("/etc/hosts")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The socket takes no bearer header, so its ticket, Host, and Origin are its whole guard.
    #[tokio::test]
    async fn the_socket_needs_a_fresh_ticket_the_host_and_the_app_origin() {
        use super::super::test_support::Fixture;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut fixture = Fixture::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        Arc::get_mut(&mut fixture.app).unwrap().port = port;
        let app = fixture.app.clone();
        tokio::spawn(async move { axum::serve(listener, router(app)).await });
        let file = fixture.dir.join("b1.output");
        std::fs::write(&file, "out\n").unwrap();

        let status = |request: String| async move {
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut head = [0u8; 12];
            stream.read_exact(&mut head).await.unwrap();
            String::from_utf8_lossy(&head[9..12]).to_string()
        };
        let origin = format!("http://127.0.0.1:{port}");
        let open = |ticket: &str, host: &str, origin: Option<&str>| {
            format!(
                "GET /api/terminal?ticket={ticket} HTTP/1.1\r\nHost: {host}\r\n{}Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: AAAAAAAAAAAAAAAAAAAAAA==\r\n\r\n",
                origin
                    .map(|o| format!("Origin: {o}\r\n"))
                    .unwrap_or_default()
            )
        };
        let host = format!("127.0.0.1:{port}");
        let ticket = mint(Target::File(file)).unwrap();

        assert_eq!(status(open("guess", &host, Some(&origin))).await, "401");
        assert_eq!(status(open(&ticket, &host, None)).await, "403");
        assert_eq!(
            status(open(&ticket, &host, Some("http://evil.test"))).await,
            "403"
        );
        assert_eq!(
            status(open(&ticket, "evil.test", Some(&origin))).await,
            "403"
        );
        assert_eq!(status(open(&ticket, &host, Some(&origin))).await, "101");
        assert_eq!(status(open(&ticket, &host, Some(&origin))).await, "401");
        let post = format!(
            "POST /api/terminal/ticket HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"
        );
        assert_eq!(status(post).await, "401");
    }

    /// The shipped tmux-swarm `attach`, pointed at a private tmux server, in a PTY: keys reach the
    /// pane, and closing the PTY ends the client and puts the window back at its size.
    #[test]
    fn attach_in_a_pty_types_into_the_pane_and_restores_its_size() {
        if std::process::Command::new("tmux")
            .arg("-V")
            .output()
            .is_err()
        {
            eprintln!("skipped: no tmux");
            return;
        }
        let server = format!("swarm-term-{}", std::process::id());
        let tmux = |args: &str| {
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("tmux -L {server} -f /dev/null {args}"))
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        tmux("new-session -d -s t -x 120 -y 40 cat");
        let pane = tmux("display -p -t t '#{pane_id}'");
        let root = std::env::temp_dir().join(format!("swarm-term-root-{}", std::process::id()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        let line = swarm::adapter::shipped("tmux-swarm")
            .unwrap()
            .lines()
            .find(|line| line.starts_with("attach = "))
            .unwrap()
            .replace("tmux -L swarm ", &format!("tmux -L {server} -f /dev/null "));
        std::fs::write(root.join("adapters/tmux-swarm.conf"), format!("{line}\n")).unwrap();

        let pair = native_pty_system().openpty(size(50, 12)).unwrap();
        let child = pair
            .slave
            .spawn_command(attach_command(&root, "tmux-swarm", &pane).unwrap())
            .unwrap();
        drop(pair.slave);
        let mut writer = pair.master.take_writer().unwrap();
        let wait_for = |what: &dyn Fn() -> bool| {
            let until = Instant::now() + Duration::from_secs(5);
            while !what() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(50));
            }
            what()
        };
        assert!(wait_for(
            &|| tmux("display -p -t t '#{session_attached}'") == "1"
        ));
        assert_eq!(
            tmux("display -p -t t '#{window_width}x#{window_height}'"),
            "50x11"
        );
        writer.write_all(b"typed-through\r").unwrap();
        assert!(wait_for(
            &|| tmux("capture-pane -p -t t").contains("typed-through")
        ));

        drop(writer);
        drop(pair.master);
        let child = Mutex::new(child);
        assert!(wait_for(&|| matches!(
            child.lock().unwrap().try_wait(),
            Ok(Some(_))
        )));
        assert_eq!(
            tmux("display -p -t t '#{window_width}x#{window_height} #{session_attached}'"),
            "120x40 0"
        );
        tmux("kill-server");
        let _ = std::fs::remove_dir_all(&root);
    }
}
