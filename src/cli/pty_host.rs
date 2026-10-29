//! The chair's `swarm pty --host` keeps its child agents on PTYs it owns, with no tmux pane per
//! child (ADR 0126). The tmux-swarm adapter sends a `pty:` pane's verbs to `swarm pty <verb>`, the
//! client half here, which talks to the host over a Unix socket. A child dies with the
//! host, and the owner accepted that.

mod attach;
mod client;
mod pane;
mod screen;

pub(crate) use client::client;
use pane::spawn;
use screen::{capture, key_bytes, screen};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A request is a few ids and a ring's text; more than this is no client of ours.
const MAX_REQUEST: u64 = 1 << 20;

/// A host answers at once, so a request that takes longer meets a stuck host or a stuck client.
pub(super) const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// A start holds the folder lock for a few ms. A holder that is stopped must not stop every later
/// chair, so after this wait a chair runs with no host.
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
const LOCK_POLL: std::time::Duration = std::time::Duration::from_millis(20);
/// How long a stopping host keeps its terminals open for its shells to pass on the hangup.
const HANG_UP_WAIT: std::time::Duration = std::time::Duration::from_secs(1);
/// Only this user reads the socket folder and the socket.
const PRIVATE_DIR: u32 = 0o700;
const PRIVATE_FILE: u32 = 0o600;

struct Hosted {
    master: std::fs::File,
    /// Input and query replies for the child, which one thread per pane writes in order. A child
    /// that reads no input fills its terminal, and then only that thread waits.
    input: std::sync::mpsc::SyncSender<Vec<u8>>,
    /// The shell leads its own session, so its pid is also its process group.
    group: libc::pid_t,
    parser: Arc<Mutex<vt100::Parser>>,
    activity: Arc<AtomicU64>,
    viewers: attach::Viewers,
}

type Children = Arc<Mutex<HashMap<u64, Hosted>>>;

/// A panic in one pane's thread, such as in the parser, must not stop the other panes, the
/// hang-up, or the chair's terminal restore, so a poisoned lock is still used.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub(crate) struct Host {
    socket: std::path::PathBuf,
    /// The bound socket's device and inode. A host that started after this one stopped answering
    /// can own the file at this path, and this host must not remove that one.
    bound: (u64, u64),
    children: Children,
}

/// The host socket of `session`, in its checked private folder.
fn socket_of(session: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    super::valid_session_id(session)?;
    socket_dir()?;
    // SAFETY: getuid cannot fail.
    swarm::bus::pty_socket_of(session, unsafe { libc::getuid() })
}

/// `/tmp/swarm-<uid>`, made 0700 when missing. Another user could make it first and listen in
/// it, so a folder that is a link, is not ours, or is open to others is refused.
pub(super) fn socket_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    // SAFETY: getuid cannot fail.
    let uid = unsafe { libc::getuid() };
    let dir = std::path::PathBuf::from(format!("/tmp/swarm-{uid}"));
    let _ = std::fs::DirBuilder::new().mode(PRIVATE_DIR).create(&dir);
    let meta = std::fs::symlink_metadata(&dir)?;
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o777 != PRIVATE_DIR {
        return Err(format!(
            "swarm pty: {} is not a private folder of this user",
            dir.display()
        )
        .into());
    }
    Ok(dir)
}

impl Host {
    /// Binds the session's socket and serves it on a thread. The caller sets `SWARM_PTY_HOST` to
    /// the returned host's socket in the chair's env.
    pub(crate) fn start() -> Result<Host, Box<dyn std::error::Error>> {
        use std::os::unix::fs::MetadataExt;
        let session = std::env::var("SWARM_SESSION_ID")
            .map_err(|_| "SWARM_SESSION_ID is not set, so no session owns the children")?;
        let socket = socket_of(&session)?;
        let dir = socket_dir()?;
        // Hosts start one at a time, so a socket that refuses connections here belongs to a host
        // that was killed, as a closed tmux pane kills it, not to one between bind and listen.
        let folder = std::fs::File::open(&dir)?;
        let deadline = std::time::Instant::now() + LOCK_WAIT;
        // SAFETY: flock on an fd this process owns; closing `folder` releases it.
        while unsafe {
            libc::flock(
                std::os::fd::AsRawFd::as_raw_fd(&folder),
                libc::LOCK_EX | libc::LOCK_NB,
            )
        } != 0
        {
            let error = std::io::Error::last_os_error();
            match error.kind() {
                std::io::ErrorKind::WouldBlock if std::time::Instant::now() > deadline => {
                    return Err(format!("another host start holds {}", dir.display()).into());
                }
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted => {}
                _ => return Err(error.into()),
            }
            std::thread::sleep(LOCK_POLL);
        }
        // A live host here belongs to another chair of the session. Taking its socket would leave
        // that chair's children unreachable, so this chair runs with no host instead.
        if UnixStream::connect(&socket).is_ok() {
            return Err(format!("{} already serves this session", socket.display()).into());
        }
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if UnixStream::connect(&path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionRefused)
            {
                let _ = std::fs::remove_file(&path);
            }
        }
        let listener = UnixListener::bind(&socket)?;
        drop(folder);
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(PRIVATE_FILE))?;
        let meta = std::fs::metadata(&socket)?;
        let children = Children::default();
        let served = children.clone();
        // Each host numbers from its start time in ms, so a pane id that an earlier chair of the
        // session stored never names a child of this one.
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |since| since.as_millis() as u64);
        let next = Arc::new(AtomicU64::new(start));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    // Such as no fd left; a short wait keeps a lasting error from spinning.
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                };
                let (children, next, session) = (served.clone(), next.clone(), session.clone());
                std::thread::spawn(move || serve(stream, &children, &next, &session));
            }
        });
        Ok(Host {
            socket,
            bound: (meta.dev(), meta.ino()),
            children,
        })
    }

    pub(crate) fn socket(&self) -> &std::path::Path {
        &self.socket
    }
}

/// How long a headless host waits for its first pane, which the launch that started it spawns.
const FIRST_PANE_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// `swarm pty host`: a host with no chair terminal of its own, which `launch orchestrator` starts
/// for a chat (ADR 0131). It prints its socket once it listens. The chair is its first pane, and
/// when the chair ends the host ends and hangs up the chair's children, as ADR 0126 has them.
pub(crate) fn headless() -> i32 {
    let host = match Host::start() {
        Ok(host) => host,
        Err(error) => {
            eprintln!("swarm pty host: {error}");
            return 1;
        }
    };
    println!("{}", host.socket.display());
    // The launch reads one line, so the pipe closes here and a later print cannot fail on it.
    // SAFETY: dup2 of a fresh /dev/null fd over stdout.
    if let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") {
        let _ = std::io::stdout().flush();
        unsafe { libc::dup2(std::os::fd::AsRawFd::as_raw_fd(&null), 1) };
    }
    let started = std::time::Instant::now();
    // The chair is the lowest pane once one runs; a spawn that failed used up its number.
    let mut chair = None;
    loop {
        std::thread::sleep(LOCK_POLL * 10);
        let children = lock(&host.children);
        chair = chair.or_else(|| children.keys().min().copied());
        let gone = chair.is_none_or(|n| !children.contains_key(&n));
        if gone && (chair.is_some() || started.elapsed() > FIRST_PANE_WAIT) {
            return 0;
        }
    }
}

/// The socket of `session`'s host, after starting a headless host when none answers there. The
/// host leads its own session, so it outlives the launch and a serve restart (ADR 0131).
pub(crate) fn ensure(session: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use std::io::BufRead;
    use std::os::unix::process::CommandExt;
    let socket = socket_of(session)?;
    if UnixStream::connect(&socket).is_ok() {
        return Ok(socket);
    }
    let runs = swarm::paths::root_dir()?.join(format!("runs/{session}"));
    std::fs::create_dir_all(&runs)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(runs.join("pty-host.log"))?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .args(["pty", "host"])
        .env("SWARM_SESSION_ID", session)
        .env_remove("SWARM_PTY_HOST")
        .env_remove("SWARM_PTY_PANE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(log);
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    // A host that cannot start exits, which closes the pipe, and its start waits at most
    // LOCK_WAIT, so this read ends.
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().expect("piped stdout")).read_line(&mut line)?;
    if std::path::Path::new(line.trim()) != socket {
        let _ = child.wait();
        // A launch of the same chat at the same time started the host first.
        if UnixStream::connect(&socket).is_ok() {
            return Ok(socket);
        }
        return Err(format!(
            "swarm pty: the chat's host did not start; see {}",
            runs.join("pty-host.log").display()
        )
        .into());
    }
    Ok(socket)
}

impl Drop for Host {
    /// Hangs up every child, as tmux does when it closes a pane. The kernel hangs them up too when
    /// this process dies without running this.
    fn drop(&mut self) {
        use std::os::unix::fs::MetadataExt;
        if std::fs::metadata(&self.socket).is_ok_and(|meta| (meta.dev(), meta.ino()) == self.bound)
        {
            let _ = std::fs::remove_file(&self.socket);
        }
        let children = std::mem::take(&mut *lock(&self.children));
        for hosted in children.values() {
            hang_up(hosted);
        }
        // bash passes the hangup on to its background jobs only while its terminal is open; a
        // master closed at once can let it exit on end of input first and leave them running.
        let deadline = std::time::Instant::now() + HANG_UP_WAIT;
        for hosted in children.values() {
            while !exited(hosted.group) && std::time::Instant::now() < deadline {
                std::thread::sleep(LOCK_POLL);
            }
        }
    }
}

/// Whether the shell has exited, without reaping it, as its pane's thread waits for it.
fn exited(pid: libc::pid_t) -> bool {
    // SAFETY: waitid writes only the zeroed siginfo it is given.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &raw mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        ) != 0
            || info.si_pid() != 0
    }
}

/// Sends SIGHUP to the shell and to the job in the terminal's foreground, which an interactive
/// shell puts in a group of its own.
fn hang_up(hosted: &Hosted) {
    use std::os::fd::AsRawFd;
    // SAFETY: tcgetpgrp and kill take plain integers.
    unsafe {
        let foreground = libc::tcgetpgrp(hosted.master.as_raw_fd());
        if foreground > 0 && foreground != hosted.group {
            libc::kill(-foreground, libc::SIGHUP);
        }
        libc::kill(-hosted.group, libc::SIGHUP);
    }
}

fn serve(stream: UnixStream, children: &Children, next: &AtomicU64, session: &str) {
    let _ = stream.set_write_timeout(Some(REQUEST_TIMEOUT));
    let request = read_request(&stream)
        .map_err(|error| error.to_string())
        .and_then(|line| {
            serde_json::from_str::<serde_json::Value>(&line).map_err(|error| error.to_string())
        });
    // A viewer keeps its connection, and the first line of its stream is the reply.
    if let Ok(request) = &request
        && request["op"] == "attach"
    {
        let error = match stream.try_clone() {
            Ok(viewer) => match attach::attach(viewer, request, children) {
                Ok(()) => return,
                Err(error) => error,
            },
            Err(error) => error.to_string(),
        };
        let reply = serde_json::json!({"ok": false, "error": error});
        let _ = writeln!(&stream, "{reply}");
        return;
    }
    let reply = request.and_then(|request| handle(&request, children, next, session));
    let reply = match reply {
        Ok(out) => serde_json::json!({"ok": true, "out": out}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    };
    let _ = writeln!(&stream, "{reply}");
}

/// One request line, read under one deadline for the whole line, so a client that sends a byte at a
/// time holds no thread for longer.
fn read_request(mut stream: &UnixStream) -> std::io::Result<String> {
    let deadline = std::time::Instant::now() + REQUEST_TIMEOUT;
    let mut line = Vec::new();
    let mut chunk = [0; 4096];
    while !line.contains(&b'\n') {
        let left = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or(std::io::ErrorKind::TimedOut)?;
        stream.set_read_timeout(Some(left))?;
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        line.extend_from_slice(&chunk[..read]);
        if line.len() as u64 > MAX_REQUEST {
            return Err(std::io::Error::other("swarm pty: request too long"));
        }
    }
    String::from_utf8(line).map_err(std::io::Error::other)
}

fn handle(
    request: &serde_json::Value,
    children: &Children,
    next: &AtomicU64,
    session: &str,
) -> Result<String, String> {
    let text = |key: &str| {
        request[key]
            .as_str()
            .ok_or_else(|| format!("swarm pty: request has no {key}"))
    };
    let op = text("op")?;
    if op == "spawn" {
        let env: HashMap<String, String> = serde_json::from_value(request["env"].clone())
            .map_err(|error| format!("swarm pty: bad env: {error}"))?;
        let n = next.fetch_add(1, Ordering::Relaxed);
        let pane = format!("pty:{session}:{n}");
        spawn(
            &pane,
            text("cwd")?,
            &env,
            text("feed")?,
            children.clone(),
            n,
        )
        .map_err(|error| format!("swarm pty: spawn: {error}"))?;
        return Ok(pane);
    }
    if op == "list" {
        let now = super::unix_now().map_or(0, |now| now as u64);
        return Ok(lock(children)
            .iter()
            .map(|(n, hosted)| {
                let active = hosted.activity.load(Ordering::Relaxed).min(now);
                format!("pty:{session}:{n} - activity={active}\n")
            })
            .collect());
    }
    let n = request["n"].as_u64().ok_or("swarm pty: request has no n")?;
    let mut children = lock(children);
    let hosted = children
        .get(&n)
        .ok_or_else(|| format!("swarm pty: no pane pty:{session}:{n}"))?;
    let bytes = match op {
        "write" => text("text")?.as_bytes().to_vec(),
        "key" => {
            let cursor = lock(&hosted.parser).screen().application_cursor();
            key_bytes(text("key")?, cursor)
        }
        "screen" => return Ok(screen(&lock(&hosted.parser))),
        "capture" => return Ok(capture(&mut lock(&hosted.parser))),
        "resize" => {
            attach::resize_op(request, hosted);
            return Ok(String::new());
        }
        "close" => {
            hang_up(&children.remove(&n).expect("found above"));
            return Ok(String::new());
        }
        _ => return Err(format!("swarm pty: unknown op {op}")),
    };
    hosted
        .input
        .try_send(bytes)
        .map(|()| String::new())
        .map_err(|error| match error {
            std::sync::mpsc::TrySendError::Full(_) => {
                format!("swarm pty: pane pty:{session}:{n} reads no input")
            }
            std::sync::mpsc::TrySendError::Disconnected(_) => {
                format!("swarm pty: pane pty:{session}:{n} takes no input")
            }
        })
}
