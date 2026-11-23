// Child processes with a deadline.

use std::io::Read;
use std::process::{Child, ExitStatus};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// The stdout of `command`, or None when it fails, or runs past `timeout` and is killed.
pub fn bounded_output(
    command: std::process::Command,
    timeout: std::time::Duration,
) -> Option<String> {
    match bounded_run(command, timeout)? {
        (true, text) => Some(text),
        (false, _) => None,
    }
}

/// Whether `command` succeeded and its stdout, read as lossy UTF-8, or None when it cannot start
/// or runs past `timeout` and is killed.
pub fn bounded_run(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Option<(bool, String)> {
    let deadline = std::time::Instant::now() + timeout;
    let child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let (bytes, status) = read_stdout(child, u64::MAX, deadline);
    Some((
        status?.success(),
        String::from_utf8_lossy(&bytes.ok()?.ok()?).into_owned(),
    ))
}

/// Reads up to `limit` bytes of the child's stdout and waits for its exit, both before `deadline`.
/// The child is always killed and reaped. A status of `None` means it did not exit in time.
pub(crate) fn read_stdout(
    mut child: Child,
    limit: u64,
    deadline: Instant,
) -> (
    Result<std::io::Result<Vec<u8>>, RecvTimeoutError>,
    Option<ExitStatus>,
) {
    let (sender, receiver) = channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take(limit).read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        });
    } else {
        let _ = sender.send(Err(std::io::Error::other("child has no stdout pipe")));
    }
    let response = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let mut status = child.try_wait().ok().flatten();
    while response.is_ok() && status.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
        status = child.try_wait().ok().flatten();
    }
    let _ = child.kill();
    let _ = child.wait();
    (response, status)
}

/// Sends SIGKILL to a child process group created with `process_group(0)`.
pub(crate) fn kill_group(group: libc::pid_t) {
    // SAFETY: callers pass the ID of the group created for their child.
    unsafe { libc::killpg(group, libc::SIGKILL) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_regression_stdout_inherited_after_exit_still_has_a_deadline() {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "sleep 0.6 & printf ready"]);
        assert_eq!(
            bounded_run(command, std::time::Duration::from_millis(50)),
            None
        );
    }
}
