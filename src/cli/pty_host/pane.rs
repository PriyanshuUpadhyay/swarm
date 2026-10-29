//! One hosted pane: its login shell on a new PTY, the thread that writes its input, and the thread
//! that reads its output.

use super::attach::{Viewers, show};
use super::{Children, Hosted, hang_up, lock};
use crate::cli::pty::{CHUNK, QUERIES, QueryScanner, Reply};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::sync::Arc;

/// tmux's size for a detached session, so a CLI lays out its screen as it does in tmux-swarm.
pub(super) const ROWS: u16 = 24;
pub(super) const COLS: u16 = 80;
/// tmux's default history-limit.
const SCROLLBACK: usize = 2000;
/// Writes that wait for a child that reads no input. tmux keeps every one, but more than this
/// means the child is stuck, so this host fails the next write. It counts writes, not bytes, so a
/// pane can hold up to 64 MiB.
const INPUT_QUEUE: usize = 64;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Held from `openpty` to the end of each spawn, so no other spawn forks while a new master is
/// still open across exec.
static SPAWN: Mutex<()> = Mutex::new(());

/// Starts the owner's login shell on a new PTY, as tmux starts a pane, adds it to `children`, and
/// reads its output on a thread until every process in it closes the terminal.
pub(super) fn spawn(
    pane: &str,
    cwd: &str,
    env: &HashMap<String, String>,
    feed: &str,
    children: Children,
    n: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let _spawning = lock(&SPAWN);
    // The pipe-pane line of tmux-swarm, so the child's OSC 7501 reports reach the store. It
    // starts first, so a failure leaves no shell behind.
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
    let stop_feed = |mut feed: std::process::Child| {
        let _ = feed.kill();
        let _ = feed.wait();
    };
    let (master, slave) = match open_pty() {
        Ok(pair) => pair,
        Err(error) => {
            stop_feed(feed);
            return Err(error);
        }
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
        .env("SWARM_PTY_PANE", pane);
    let started = (|| {
        let writer = (
            std::fs::File::from(master.try_clone()?),
            std::fs::File::from(master.try_clone()?),
        );
        command
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
        command.spawn().map(|child| (writer, child))
    })();
    let ((master_file, mut writer), mut child) = match started {
        Ok(started) => started,
        Err(error) => {
            stop_feed(feed);
            return Err(error.into());
        }
    };
    let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, SCROLLBACK)));
    let activity = Arc::new(AtomicU64::new(
        crate::cli::unix_now().map_or(0, |now| now as u64),
    ));
    let (input, queued) = std::sync::mpsc::sync_channel::<Vec<u8>>(INPUT_QUEUE);
    let replies = input.clone();
    let viewers = Viewers::default();
    // It ends when the pane leaves the list and the reader ends, which drop the senders, or when
    // the child is gone.
    std::thread::spawn(move || {
        for bytes in queued {
            if writer.write_all(&bytes).is_err() {
                break;
            }
        }
    });
    // In the list before the reader runs, so a shell that exits at once is removed after it.
    lock(&children).insert(
        n,
        Hosted {
            master: master_file,
            input,
            group: child.id() as libc::pid_t,
            parser: parser.clone(),
            activity: activity.clone(),
            viewers: viewers.clone(),
        },
    );
    std::thread::spawn(move || {
        let mut feed_in = feed_in;
        // A panic, such as in the parser, must still take the pane off the list and reap it.
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_pane(master, &mut feed_in, &parser, &activity, &replies, &viewers)
        }));
        // The pane is gone once its shell and everything it left have exited, as in tmux.
        let gone = lock(&children).remove(&n);
        if let (Err(_), Some(hosted)) = (read, &gone) {
            hang_up(hosted);
        }
        drop((gone, replies, feed_in));
        let _ = child.wait();
        let _ = feed.wait();
    });
    Ok(())
}

/// Reads a pane's output into its parser and its feed until every process in the terminal has
/// closed it, and queues each query's reply.
fn read_pane(
    master: OwnedFd,
    feed_in: &mut std::process::ChildStdin,
    parser: &Mutex<vt100::Parser>,
    activity: &AtomicU64,
    replies: &std::sync::mpsc::SyncSender<Vec<u8>>,
    viewers: &Viewers,
) {
    let mut terminal = std::fs::File::from(master);
    let mut scanner = QueryScanner::new(QUERIES);
    let mut chunk = [0; CHUNK];
    // A read error is EIO once every process in the terminal has closed it.
    loop {
        let read = match terminal.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let bytes = &chunk[..read];
        if let Ok(now) = crate::cli::unix_now() {
            activity.store(now as u64, Ordering::Relaxed);
        }
        let _ = feed_in.write_all(bytes);
        let mut parser = lock(parser);
        let mut done = 0;
        // A viewer's terminal would answer each query again, into the pane, so viewers get the
        // bytes without them.
        let mut shown = Vec::with_capacity(bytes.len());
        // Each reply follows the bytes before its query, so a cursor report names the cell the
        // cursor was on when the CLI asked.
        for (end, query) in scanner.push(bytes) {
            parser.process(&bytes[done..end]);
            shown.extend_from_slice(&bytes[done..end.saturating_sub(query.text.len()).max(done)]);
            done = end;
            let reply = match query.reply {
                Reply::Text(text) => text.to_vec(),
                Reply::Cursor => {
                    let (row, col) = parser.screen().cursor_position();
                    format!("\x1b[{};{}R", row + 1, col + 1).into_bytes()
                }
            };
            // The writer thread sends it, so this thread never waits on a child that reads no
            // input, and goes on reading what the child prints. A full queue drops it, as that
            // child would not read it.
            let _ = replies.try_send(reply);
        }
        parser.process(&bytes[done..]);
        shown.extend_from_slice(&bytes[done..]);
        show(viewers, &shown);
    }
    // Each viewer's writer ends when its queue goes, and closes the viewer's stream.
    lock(viewers).clear();
}

/// A new PTY at tmux's detached size, with both ends closed on exec, so no other child holds
/// this one's terminal open.
fn open_pty() -> Result<(OwnedFd, OwnedFd), Box<dyn std::error::Error>> {
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
            &raw mut size,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: openpty succeeded, so both fds are open and owned here alone.
    unsafe {
        libc::fcntl(master_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        Ok((
            OwnedFd::from_raw_fd(master_fd),
            OwnedFd::from_raw_fd(slave_fd),
        ))
    }
}
