//! The chair's `swarm pty --host` keeps its child agents on PTYs it owns, with no tmux pane per
//! child (ADR 0126). The tmux-swarm adapter sends a `pty:` pane's verbs to `swarm pty <verb>`, the
//! client half here, which talks to the host over a Unix socket. A child dies with the
//! host, and the owner accepted that.

mod client;
mod screen;

use super::pty::{PROBE_REPLY, QUERIES, Scanner};
pub(crate) use client::client;
use screen::{capture, key_bytes, screen};
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// tmux's size for a detached session, so a CLI lays out its screen as it does in tmux-swarm.
const ROWS: u16 = 24;
const COLS: u16 = 80;
/// tmux's default history-limit.
const SCROLLBACK: usize = 2000;
/// A request is a few ids and a ring's text; more than this is no client of ours.
const MAX_REQUEST: u64 = 1 << 20;

struct Hosted {
    writer: std::fs::File,
    /// The shell leads its own session, so its pid is also its process group.
    group: libc::pid_t,
    parser: Arc<Mutex<vt100::Parser>>,
    activity: Arc<AtomicU64>,
}

type Children = Arc<Mutex<HashMap<u64, Hosted>>>;

pub(crate) struct Host {
    socket: std::path::PathBuf,
    children: Children,
}

/// The host socket of `session`. A socket path holds at most 104 bytes on macOS, and a branch
/// build's `runs/<session>/` is longer, so the socket lives in a private folder per user, as
/// tmux's does, under a hash of that runs folder.
fn socket_of(session: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use sha2::Digest;
    use std::os::unix::ffi::OsStrExt;
    super::valid_session_id(session)?;
    let runs = swarm::paths::root_dir()?.join(format!("runs/{session}"));
    let hash = sha2::Sha256::digest(runs.as_os_str().as_bytes());
    let name: String = hash[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(socket_dir()?.join(format!("{name}.sock")))
}

/// `/tmp/swarm-<uid>`, made 0700 when missing. Another user could make it first and listen in
/// it, so a folder that is a link, is not ours, or is open to others is refused.
fn socket_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    // SAFETY: getuid cannot fail.
    let uid = unsafe { libc::getuid() };
    let dir = std::path::PathBuf::from(format!("/tmp/swarm-{uid}"));
    let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
    let meta = std::fs::symlink_metadata(&dir)?;
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
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
        use std::os::unix::fs::DirBuilderExt;
        let session = std::env::var("SWARM_SESSION_ID")?;
        let socket = socket_of(&session)?;
        // `swarm pty list` finds a host by its runs folder.
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(swarm::paths::root_dir()?.join(format!("runs/{session}")))?;
        // A socket left by a host that was killed refuses connections; a live one belongs to a
        // second chair of the same session, which takes the children's socket from it.
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let children = Children::default();
        let served = children.clone();
        let next = Arc::new(AtomicU64::new(1));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (children, next, session) = (served.clone(), next.clone(), session.clone());
                std::thread::spawn(move || serve(stream, &children, &next, &session));
            }
        });
        Ok(Host { socket, children })
    }

    pub(crate) fn socket(&self) -> &std::path::Path {
        &self.socket
    }
}

impl Drop for Host {
    /// Hangs up every child, as tmux does when it closes a pane. The kernel hangs them up too when
    /// this process dies without running this.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
        let children = std::mem::take(&mut *self.children.lock().unwrap());
        for hosted in children.values() {
            hang_up(hosted);
        }
    }
}

/// Sends SIGHUP to the shell and to the job in the terminal's foreground, which an interactive
/// shell puts in a group of its own.
fn hang_up(hosted: &Hosted) {
    use std::os::fd::AsRawFd;
    // SAFETY: tcgetpgrp and kill take plain integers.
    unsafe {
        let foreground = libc::tcgetpgrp(hosted.writer.as_raw_fd());
        if foreground > 0 && foreground != hosted.group {
            libc::kill(-foreground, libc::SIGHUP);
        }
        libc::kill(-hosted.group, libc::SIGHUP);
    }
}

fn serve(stream: UnixStream, children: &Children, next: &AtomicU64, session: &str) {
    let mut line = String::new();
    let reply = match std::io::BufReader::new((&stream).take(MAX_REQUEST)).read_line(&mut line) {
        Ok(_) => serde_json::from_str(&line)
            .map_err(|error| error.to_string())
            .and_then(|request| handle(&request, children, next, session)),
        Err(error) => Err(error.to_string()),
    };
    let reply = match reply {
        Ok(out) => serde_json::json!({"ok": true, "out": out}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    };
    let _ = writeln!(&stream, "{reply}");
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
        let hosted = spawn(
            &pane,
            text("cwd")?,
            &env,
            text("feed")?,
            children.clone(),
            n,
        )
        .map_err(|error| format!("swarm pty: spawn: {error}"))?;
        children.lock().unwrap().insert(n, hosted);
        return Ok(pane);
    }
    if op == "list" {
        let now = unix_now();
        return Ok(children
            .lock()
            .unwrap()
            .iter()
            .map(|(n, hosted)| {
                let quiet = now.saturating_sub(hosted.activity.load(Ordering::Relaxed));
                format!("pty:{session}:{n} - activity={}\n", now - quiet)
            })
            .collect());
    }
    let n = request["n"].as_u64().ok_or("swarm pty: request has no n")?;
    let mut children = children.lock().unwrap();
    let hosted = children
        .get_mut(&n)
        .ok_or_else(|| format!("swarm pty: no pane pty:{session}:{n}"))?;
    match op {
        "write" => hosted
            .writer
            .write_all(text("text")?.as_bytes())
            .map(|()| String::new())
            .map_err(|error| error.to_string()),
        "key" => {
            let cursor = hosted.parser.lock().unwrap().screen().application_cursor();
            let bytes = key_bytes(text("key")?, cursor)?;
            hosted
                .writer
                .write_all(&bytes)
                .map(|()| String::new())
                .map_err(|error| error.to_string())
        }
        "screen" => Ok(screen(&hosted.parser.lock().unwrap())),
        "capture" => Ok(capture(&mut hosted.parser.lock().unwrap())),
        "close" => {
            let hosted = children.remove(&n).expect("found above");
            hang_up(&hosted);
            Ok(String::new())
        }
        _ => Err(format!("swarm pty: unknown op {op}")),
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Starts the owner's login shell on a new PTY, as tmux starts a pane, and reads its output on a
/// thread until every process in it closes the terminal.
fn spawn(
    pane: &str,
    cwd: &str,
    env: &HashMap<String, String>,
    feed: &str,
    children: Children,
    n: u64,
) -> Result<Hosted, Box<dyn std::error::Error>> {
    let (mut master_fd, mut slave_fd) = (-1, -1);
    let mut size = libc::winsize {
        ws_row: ROWS,
        ws_col: COLS,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: openpty writes two fds; the name pointer and termp may be null.
    if unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: openpty succeeded, so both fds are open and owned here alone. Close-on-exec keeps
    // the master out of every other child, which would otherwise hold this one's terminal open.
    let (master, slave) = unsafe {
        libc::fcntl(master_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        (
            OwnedFd::from_raw_fd(master_fd),
            OwnedFd::from_raw_fd(slave_fd),
        )
    };
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".into());
    let name = std::path::Path::new(&shell)
        .file_name()
        .map_or("sh".into(), |name| name.to_string_lossy().into_owned());
    let mut command = std::process::Command::new(&shell);
    // A leading dash makes it a login shell, as tmux starts one.
    command
        .arg0(format!("-{name}"))
        .current_dir(cwd)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("SWARM_PTY_HOST")
        .envs(env)
        .env("SWARM_PTY_PANE", pane)
        .stdin(std::process::Stdio::from(slave.try_clone()?))
        .stdout(std::process::Stdio::from(slave.try_clone()?))
        .stderr(std::process::Stdio::from(slave));
    // SAFETY: the closure calls only setsid and ioctl, which are async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    // The pipe-pane line of tmux-swarm, so the child's OSC 7501 reports reach the store.
    let mut feed = std::process::Command::new("sh")
        .args(["-c", feed])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let feed_in = feed.stdin.take().expect("piped stdin");
    // A stalled feed must not stop the child, so a full pipe drops the bytes, as status-feed
    // already accepts a lost report.
    // SAFETY: fcntl on an fd this process owns.
    unsafe {
        use std::os::fd::AsRawFd;
        let fd = feed_in.as_raw_fd();
        libc::fcntl(
            fd,
            libc::F_SETFL,
            libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK,
        );
    }
    let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, SCROLLBACK)));
    let activity = Arc::new(AtomicU64::new(unix_now()));
    let hosted = Hosted {
        writer: std::fs::File::from(master.try_clone()?),
        group: child.id() as libc::pid_t,
        parser: parser.clone(),
        activity: activity.clone(),
    };
    std::thread::spawn(move || {
        let mut terminal = std::fs::File::from(master);
        let mut feed_in = feed_in;
        let mut scanner = Scanner::new(&QUERIES);
        let mut chunk = [0; 65536];
        // A read error is EIO once every process in the terminal has closed it.
        loop {
            let read = match terminal.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            let bytes = &chunk[..read];
            activity.store(unix_now(), Ordering::Relaxed);
            let _ = feed_in.write_all(bytes);
            let mut parser = parser.lock().unwrap();
            let mut done = 0;
            // Each reply follows the bytes before its query, so a cursor report names the cell
            // the cursor was on when the CLI asked.
            for (end, query) in scanner.push(bytes) {
                parser.process(&bytes[done..end]);
                done = end;
                let reply = match query {
                    0 => PROBE_REPLY.to_vec(),
                    1 | 2 => b"\x1b[?1;2c".to_vec(),
                    3 => b"\x1b[0n".to_vec(),
                    4 => {
                        let (row, col) = parser.screen().cursor_position();
                        format!("\x1b[{};{}R", row + 1, col + 1).into_bytes()
                    }
                    5 => b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\".to_vec(),
                    _ => b"\x1b]11;rgb:0000/0000/0000\x1b\\".to_vec(),
                };
                let _ = terminal.write_all(&reply);
            }
            parser.process(&bytes[done..]);
        }
        // The pane is gone once its shell and everything it left have exited, as in tmux.
        children.lock().unwrap().remove(&n);
        drop(feed_in);
        let _ = child.wait();
        let _ = feed.wait();
    });
    Ok(hosted)
}
