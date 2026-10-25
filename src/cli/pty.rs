//! `swarm pty -- <cmd>`: runs an agent CLI on a terminal that swarm owns, inside its tmux pane,
//! and answers the OSC 7501 probe there (ADR 0122). tmux does not answer it, and it answers the
//! device-attributes query that a CLI sends right after the probe, so a CLI in a bare tmux pane
//! decides that no program-status reader is there and sends no reports.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::sync::atomic::{AtomicI32, Ordering};

const PROBE: &[u8] = b"\x1b]7501;?";
/// A supporting terminal replies with `?`; no `states` or `kinds` means the base sets.
const PROBE_REPLY: &[u8] = b"\x1b]7501;?\x07";

/// The PTY master, for the SIGWINCH handler.
static MASTER: AtomicI32 = AtomicI32::new(-1);

/// Counts the probes in a byte stream that arrives in pieces.
#[derive(Default)]
pub(crate) struct ProbeScanner {
    tail: Vec<u8>,
}

impl ProbeScanner {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> usize {
        self.tail.extend_from_slice(bytes);
        let found = self
            .tail
            .windows(PROBE.len())
            .filter(|window| *window == PROBE)
            .count();
        // Keep a tail that can hold the start of a split probe, but no whole probe.
        let keep = self.tail.len().min(PROBE.len() - 1);
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
/// cannot start. Bytes pass through unchanged in both directions.
pub(crate) fn pty(command: &[String]) -> i32 {
    let Some((program, args)) = command.split_first() else {
        eprintln!("usage: swarm pty -- <cmd> [args...]");
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
    // SAFETY: openpty succeeded, so both fds are open and owned here alone.
    let (master, slave) = unsafe {
        (
            OwnedFd::from_raw_fd(master_fd),
            OwnedFd::from_raw_fd(slave_fd),
        )
    };
    let stdio = || slave.try_clone().map(std::process::Stdio::from);
    let child = match (stdio(), stdio(), stdio()) {
        (Ok(stdin), Ok(stdout), Ok(stderr)) => {
            let mut child = std::process::Command::new(program);
            child.args(args).stdin(stdin).stdout(stdout).stderr(stderr);
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
    }
    // Raw mode, so each key reaches the CLI as it is typed and Ctrl-C reaches it as a byte.
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: tcgetattr fills a local termios; cfmakeraw and tcsetattr read it.
    let raw = unsafe { libc::tcgetattr(0, &mut saved) } == 0 && {
        let mut raw = saved;
        unsafe { libc::cfmakeraw(&mut raw) };
        (unsafe { libc::tcsetattr(0, libc::TCSANOW, &raw) }) == 0
    };

    let (Ok(input), Ok(output)) = (master.try_clone(), master.try_clone()) else {
        eprintln!("swarm pty: {}", std::io::Error::last_os_error());
        return 127;
    };
    std::thread::spawn(move || {
        let mut input = std::fs::File::from(input);
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut input);
    });
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut terminal = std::fs::File::from(output);
        let mut scanner = ProbeScanner::default();
        let mut stdout = std::io::stdout().lock();
        let mut chunk = [0; 65536];
        // A read error is EIO once the CLI and every process it left have closed the terminal.
        while let Ok(read @ 1..) = terminal.read(&mut chunk) {
            // Answer before this chunk reaches tmux, so the CLI reads the reply before tmux's
            // device-attributes reply to the query that follows the probe.
            for _ in 0..scanner.push(&chunk[..read]) {
                let _ = terminal.write_all(PROBE_REPLY);
            }
            if stdout
                .write_all(&chunk[..read])
                .and_then(|()| stdout.flush())
                .is_err()
            {
                break;
            }
        }
        let _ = done.send(());
    });

    let status = child.wait();
    // A process the CLI left behind can keep the terminal open, so the last output gets a moment.
    let _ = finished.recv_timeout(std::time::Duration::from_secs(1));
    if raw {
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

    #[test]
    fn finds_a_probe_split_across_reads() {
        let mut scanner = ProbeScanner::default();
        assert_eq!(scanner.push(b"text\x1b]75"), 0);
        assert_eq!(scanner.push(b"01;?\x1b\\\x1b[c"), 1);
        assert_eq!(scanner.push(b"\x1b]7501;state=idle\x07"), 0);
    }

    #[test]
    fn counts_each_probe_once() {
        let mut scanner = ProbeScanner::default();
        assert_eq!(scanner.push(b"\x1b]7501;?\x07\x1b]7501;?\x07"), 2);
        assert_eq!(scanner.push(b"\x07"), 0);
    }
}
