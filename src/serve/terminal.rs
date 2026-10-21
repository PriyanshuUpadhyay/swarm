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
/// A live terminal ends only when its viewer leaves, so this is a guess, not a promise.
const RETRY_TERMINAL: Duration = Duration::from_secs(5);
/// How much of a shell's output file a new terminal shows first.
const FILE_TAIL: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Target {
    Attach { adapter: String, pane: String },
    File(PathBuf),
}

static TICKETS: Mutex<Vec<(String, Instant, Target)>> = Mutex::new(Vec::new());
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// A 429 that says when to try again.
fn busy(message: &str, retry_after: Duration) -> Response {
    let mut response = error(StatusCode::TOO_MANY_REQUESTS, message).into_response();
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, retry_after.as_secs().into());
    response
}

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
) -> Result<Json<Value>, Response> {
    let target = tokio::task::spawn_blocking(move || target(&app, &request))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "ticket failed"))
        .flatten()
        .map_err(IntoResponse::into_response)?;
    let ticket = mint(target).map_err(|err| match err.0 {
        // The oldest pending ticket is gone by then.
        StatusCode::TOO_MANY_REQUESTS => busy("too many terminal tickets", TICKET_TTL),
        _ => err.into_response(),
    })?;
    Ok(Json(json!({ "ticket": ticket })))
}

fn target(app: &Server, request: &TicketRequest) -> Result<Target, ApiError> {
    if let Some(task) = &request.task {
        let path = session_tasks(app, &request.session)
            .into_iter()
            .find(|t| t.id == *task && t.agent == request.agent && t.kind == TaskKind::Shell)
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
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?
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
    let Some(slot) = Slot::take() else {
        return busy("too many terminals", RETRY_TERMINAL);
    };
    upgrade.on_upgrade(move |socket| async move {
        let _slot = slot;
        match target {
            Target::Attach { adapter, pane } => attach(&app.root, &adapter, &pane, socket).await,
            Target::File(path) => follow_file(path, socket).await,
        }
    })
}

/// One of the `MAX_LIVE` terminals. Dropping it gives the place back, also when the upgrade
/// fails (axum drops the callback unrun) or the terminal task panics.
struct Slot;

impl Slot {
    fn take() -> Option<Slot> {
        if LIVE.fetch_add(1, Ordering::SeqCst) >= MAX_LIVE {
            LIVE.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Slot)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
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

const SIGHUP: i32 = 1;

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

/// A writer to the PTY that closes without a key. portable-pty's own writer sends a newline and
/// EOF when it drops; the tmux client passes them on, and the agent's shell reads EOF and exits.
fn pty_writer(
    master: &dyn portable_pty::MasterPty,
) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    let fd = master.as_raw_fd().ok_or("pty has no fd")?;
    // SAFETY: `master` owns `fd` and stays alive for this call; the clone is a new owned fd.
    let owned = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?;
    Ok(owned.into())
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
            let writer = pty_writer(&*pair.master)?;
            Ok((pair.master, child, reader, writer))
        })
        .map_err(|err| err.to_string());
    let (master, child, mut reader, mut writer) = match started {
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
    drop(from_pty);
    drop(master);
    let _ = socket.send(Message::Close(None)).await;
    let _ = tokio::task::spawn_blocking(move || hang_up(child)).await;
}

/// Ends the attach child the way a closed terminal would. A reader thread can still hold the PTY
/// open, so no hangup reaches the child on its own. SIGHUP ends the tmux client, then the verb
/// restores the window size. A child that is still there after that is killed, and it is reaped.
fn hang_up(mut child: Box<dyn portable_pty::Child + Send + Sync>) {
    if let Some(pid) = child.process_id() {
        // SAFETY: kill has no memory preconditions; the PTY child leads its own process group.
        unsafe { kill(-(pid as i32), SIGHUP) };
    }
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
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
#[path = "terminal/tests.rs"]
mod tests;
