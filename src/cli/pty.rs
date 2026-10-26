//! `swarm pty -- <cmd>`: runs an agent CLI on a terminal that swarm owns, inside its tmux pane,
//! and answers the OSC 7501 probe there (ADR 0123). tmux does not answer it, and it answers the
//! device-attributes query that a CLI sends right after the probe, so a CLI in a bare tmux pane
//! decides that no program-status reader is there and sends no reports.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::sync::atomic::{AtomicI32, Ordering};

/// What a terminal sends back for a query.
pub(crate) enum Reply {
    Text(&'static [u8]),
    /// `CSI <row>;<col> R`, from the cursor at the moment of the query.
    Cursor,
}

pub(crate) struct Query {
    pub(crate) text: &'static [u8],
    pub(crate) reply: Reply,
}

/// A supporting terminal replies with `?`; no `states` or `kinds` means the base sets.
const PROBE: Query = Query {
    text: b"\x1b]7501;?",
    reply: Reply::Text(b"\x1b]7501;?\x07"),
};
/// The shim inside tmux answers only the OSC 7501 probe, because tmux answers the rest.
const PROBE_ONLY: &[Query] = &[PROBE];
/// The host has no terminal behind it, so it answers the queries the CLIs send at start. A CLI
/// that gets no cursor report can refuse to start.
pub(crate) const QUERIES: &[Query] = &[
    PROBE,
    Query {
        text: b"\x1b[c",
        reply: Reply::Text(b"\x1b[?1;2c"),
    },
    Query {
        text: b"\x1b[0c",
        reply: Reply::Text(b"\x1b[?1;2c"),
    },
    Query {
        text: b"\x1b[5n",
        reply: Reply::Text(b"\x1b[0n"),
    },
    Query {
        text: b"\x1b[6n",
        reply: Reply::Cursor,
    },
    Query {
        text: b"\x1b]10;?",
        reply: Reply::Text(b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\"),
    },
    Query {
        text: b"\x1b]11;?",
        reply: Reply::Text(b"\x1b]11;rgb:0000/0000/0000\x1b\\"),
    },
];
/// One read of a PTY master.
pub(crate) const CHUNK: usize = 65536;

/// The PTY master, for the SIGWINCH handler.
static MASTER: AtomicI32 = AtomicI32::new(-1);

/// Finds terminal queries in a byte stream that arrives in pieces.
pub(crate) struct QueryScanner {
    queries: &'static [Query],
    tail: Vec<u8>,
}

impl QueryScanner {
    pub(crate) fn new(queries: &'static [Query]) -> Self {
        QueryScanner {
            queries,
            tail: Vec::new(),
        }
    }

    /// Each query that ends in `bytes`, in order, with its end offset in `bytes`.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<(usize, &'static Query)> {
        let kept = self.tail.len();
        self.tail.extend_from_slice(bytes);
        let queries = self.queries;
        let mut found: Vec<(usize, &'static Query)> = queries
            .iter()
            .flat_map(|query| {
                let tail = &self.tail;
                (0..=tail.len().saturating_sub(query.text.len()))
                    .filter(move |&at| tail[at..].starts_with(query.text))
                    .map(move |at| (at + query.text.len(), query))
            })
            // A query that ended in the kept tail was found in an earlier push.
            .filter(|&(end, _)| end > kept)
            .map(|(end, query)| (end - kept, query))
            .collect();
        found.sort_by_key(|&(end, _)| end);
        // Keep a tail that can hold the start of a split query, but no whole query.
        let longest = queries.iter().map(|q| q.text.len()).max().unwrap_or(1);
        let keep = self.tail.len().min(longest - 1);
        self.tail.drain(..self.tail.len() - keep);
        found
    }
}

/// Copies the outer terminal's size to the PTY, which signals the CLI to redraw.
extern "C" fn copy_size(_: libc::c_int) {
    let master = MASTER.load(Ordering::Relaxed);
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: both calls take a valid pointer to a local winsize; a bad fd only returns -1.
    unsafe {
        if libc::ioctl(0, libc::TIOCGWINSZ, &mut size) == 0 {
            libc::ioctl(master, libc::TIOCSWINSZ, &size);
        }
    }
}

/// Runs `command` on a new PTY and returns its exit code, 128 + its signal, or 127 when it
/// cannot start. Bytes pass through unchanged in both directions. With `host`, it also hosts the
/// chair's child agents until `command` exits.
pub(crate) fn pty(command: &[String], host: bool) -> i32 {
    let Some((program, args)) = command.split_first() else {
        eprintln!("usage: swarm pty [--host] -- <cmd> [args...]");
        return 2;
    };
    let (mut master_fd, mut slave_fd) = (-1, -1);
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ writes a winsize; on a non-terminal stdin it fails and size stays zero,
    // which openpty treats as no size.
    let sized = unsafe { libc::ioctl(0, libc::TIOCGWINSZ, &mut size) } == 0;
    // SAFETY: openpty writes two fds; the name pointer may be null, and termp is null for defaults.
    if unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            if sized {
                &mut size
            } else {
                std::ptr::null_mut()
            },
        )
    } != 0
    {
        eprintln!("swarm pty: openpty: {}", std::io::Error::last_os_error());
        return 127;
    }
    // SAFETY: openpty succeeded, so both fds are open and owned here alone. openpty does not set
    // close-on-exec, and a CLI that inherits its own master is never hung up when this process
    // dies, so it would outlive its pane.
    let (master, slave) = unsafe {
        libc::fcntl(master_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        (
            OwnedFd::from_raw_fd(master_fd),
            OwnedFd::from_raw_fd(slave_fd),
        )
    };
    let (Ok(input), Ok(output)) = (master.try_clone(), master.try_clone()) else {
        eprintln!("swarm pty: {}", std::io::Error::last_os_error());
        return 127;
    };
    // Without a host the chair still runs, and its children open tmux panes as before.
    let child_host = host
        .then(|| {
            super::pty_host::Host::start()
                .inspect_err(|error| eprintln!("swarm pty: no child host: {error}"))
                .ok()
        })
        .flatten();
    let stdio = || slave.try_clone().map(std::process::Stdio::from);
    let child = match (stdio(), stdio(), stdio()) {
        (Ok(stdin), Ok(stdout), Ok(stderr)) => {
            let mut child = std::process::Command::new(program);
            child.args(args).stdin(stdin).stdout(stdout).stderr(stderr);
            // An inherited host is another chair's, so a chair with no host of its own drops it.
            match &child_host {
                Some(child_host) => child.env("SWARM_PTY_HOST", child_host.socket()),
                None => child.env_remove("SWARM_PTY_HOST"),
            };
            // SAFETY: the closure calls only setsid and ioctl, which are async-signal-safe.
            unsafe {
                child.pre_exec(|| {
                    if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            child.spawn()
        }
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
    };
    // The child holds its own copies; with ours closed, a read of the master ends when it exits.
    drop(slave);
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            eprintln!("swarm pty: {program}: {error}");
            return 127;
        }
    };

    MASTER.store(master.as_raw_fd(), Ordering::Relaxed);
    // SAFETY: the handler only reads an atomic and calls ioctl.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = copy_size as extern "C" fn(libc::c_int) as usize;
        action.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGWINCH, &action, std::ptr::null_mut());
        // A resize while the CLI started came before the handler, so copy the size once now.
        copy_size(0);
    }
    // Raw mode, so each key reaches the CLI as it is typed and Ctrl-C reaches it as a byte.
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: tcgetattr fills a local termios; cfmakeraw and tcsetattr read it.
    let raw_mode = unsafe { libc::tcgetattr(0, &mut saved) } == 0 && {
        let mut attrs = saved;
        unsafe { libc::cfmakeraw(&mut attrs) };
        (unsafe { libc::tcsetattr(0, libc::TCSANOW, &attrs) }) == 0
    };
    std::thread::spawn(move || {
        let mut input = std::fs::File::from(input);
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut input);
    });
    let (done, finished) = std::sync::mpsc::channel();
    // The CLI leads its own session, so its pid is also its process group.
    let group = child.id() as libc::pid_t;
    std::thread::spawn(move || {
        let mut terminal = std::fs::File::from(output);
        let mut scanner = QueryScanner::new(PROBE_ONLY);
        let mut stdout = std::io::stdout().lock();
        let mut chunk = [0; CHUNK];
        let mut output_open = true;
        // A read error is EIO once the CLI and every process it left have closed the terminal.
        loop {
            let read = match terminal.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            // Answer before this chunk reaches tmux, so the CLI reads the reply before tmux's
            // device-attributes reply to the query that follows the probe.
            for _ in scanner.push(&chunk[..read]) {
                if let Reply::Text(reply) = PROBE.reply {
                    let _ = terminal.write_all(reply);
                }
            }
            if output_open
                && stdout
                    .write_all(&chunk[..read])
                    .and_then(|()| stdout.flush())
                    .is_err()
            {
                // With no reader the CLI blocks on a full terminal, so hang it up as a pane close
                // would, and keep draining: a process cannot finish its exit while its terminal
                // holds unread output.
                output_open = false;
                // SAFETY: kill only sends a signal.
                unsafe { libc::kill(-group, libc::SIGHUP) };
            }
        }
        let _ = done.send(());
    });

    let status = child.wait();
    // A process the CLI left behind can keep the terminal open, so the last output gets a moment.
    let _ = finished.recv_timeout(std::time::Duration::from_secs(1));
    // Its children end with the chair (ADR 0126).
    drop(child_host);
    if raw_mode {
        // SAFETY: restores the termios read above.
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
    }
    match status {
        Ok(status) => status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)),
        Err(error) => {
            eprintln!("swarm pty: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(scanner: &mut QueryScanner, bytes: &[u8]) -> Vec<(usize, &'static [u8])> {
        scanner
            .push(bytes)
            .into_iter()
            .map(|(end, query)| (end, query.text))
            .collect()
    }

    #[test]
    fn finds_a_probe_split_across_reads() {
        let mut scanner = QueryScanner::new(PROBE_ONLY);
        assert_eq!(found(&mut scanner, b"text\x1b]75"), vec![]);
        assert_eq!(
            found(&mut scanner, b"01;?\x1b\\\x1b[c"),
            vec![(4, PROBE.text)]
        );
        assert_eq!(found(&mut scanner, b"\x1b]7501;state=idle\x07"), vec![]);
    }

    #[test]
    fn finds_each_query_once_in_order() {
        let mut scanner = QueryScanner::new(QUERIES);
        assert_eq!(
            found(&mut scanner, b"\x1b]7501;?\x07\x1b[c\x1b[6"),
            vec![(8, PROBE.text), (12, &b"\x1b[c"[..])]
        );
        // The DA1 query is still in the kept tail; only the cursor query is new.
        assert_eq!(found(&mut scanner, b"n"), vec![(1, &b"\x1b[6n"[..])]);
        assert_eq!(found(&mut scanner, b"\x07"), vec![]);
    }
}
