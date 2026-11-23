//! Rule process execution and bounded deny reasons.

use super::*;
use std::os::unix::process::CommandExt;

pub(super) enum Outcome {
    Allow,
    Deny(String),
    Failed(String),
}

/// Run one rule with the call on stdin, in the session's folder when it exists. Exit 0 allows,
/// exit 2 denies with stderr as the reason, and anything else is a failure.
pub(super) fn run(
    command: &[String],
    home: &str,
    cwd: Option<&std::path::Path>,
    input: &str,
    limit: Duration,
) -> Outcome {
    let command: Vec<String> = command
        .iter()
        .map(|word| match word.strip_prefix("~/") {
            Some(rest) => format!("{home}/{rest}"),
            None => word.clone(),
        })
        .collect();
    let Some((program, args)) = command.split_first() else {
        return Outcome::Failed("its command is empty".into());
    };
    if limit.is_zero() {
        return Outcome::Failed("no time left before the deadline".into());
    }
    let (mut stderr, writer) = match std::os::unix::net::UnixStream::pair() {
        Ok(pair) => pair,
        Err(error) => return Outcome::Failed(format!("cannot open stderr: {error}")),
    };
    if let Err(error) = stderr.set_nonblocking(true) {
        return Outcome::Failed(format!("cannot read stderr without blocking: {error}"));
    }
    let mut process = std::process::Command::new(program);
    if let Some(cwd) = cwd.filter(|cwd| cwd.is_dir()) {
        process.current_dir(cwd);
    }
    let mut child = match process
        .process_group(0)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::os::fd::OwnedFd::from(writer))
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return Outcome::Failed(format!("{program} did not start: {error}")),
    };
    // A rule that does not read a large payload must not stall the runner past its deadline.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = input.to_string();
    std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let mut text = Vec::new();
    let started = Instant::now();
    let status = loop {
        drain_reason(&mut stderr, &mut text);
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < limit => std::thread::sleep(POLL),
            Ok(None) => {
                stop_rule(&mut child);
                return Outcome::Failed(format!("no answer within {} s", limit.as_secs_f32()));
            }
            Err(error) => {
                stop_rule(&mut child);
                return Outcome::Failed(format!("cannot wait for it: {error}"));
            }
        }
    };
    // Read the child's final bytes directly. A reader thread can miss a short wait under load,
    // and waiting for EOF can lose the reason when a descendant inherits stderr.
    drain_reason(&mut stderr, &mut text);
    // A character the read cut in two is dropped, else it would decode to a U+FFFD.
    if text.len() as u64 == REASON_MAX
        && let Some(chunk) = text.utf8_chunks().last()
    {
        text.truncate(text.len() - chunk.invalid().len());
    }
    // Invalid bytes expand to 3-byte U+FFFDs, so cap the decoded text at a character boundary.
    let mut text = String::from_utf8_lossy(&text).into_owned();
    text.truncate(text.floor_char_boundary(REASON_MAX as usize));
    match status.code() {
        Some(0) => Outcome::Allow,
        Some(2) => Outcome::Deny(text.trim().to_string()),
        Some(code) => Outcome::Failed(format!("exit {code}")),
        None => Outcome::Failed("killed by a signal".into()),
    }
}

fn stop_rule(child: &mut std::process::Child) {
    crate::process::kill_group(child.id() as libc::pid_t);
    let _ = child.kill();
    let _ = child.wait();
}

fn drain_reason(stderr: &mut impl Read, text: &mut Vec<u8>) {
    let mut buffer = [0; 8192];
    // Bound each pass so continuous output cannot keep us from checking the deadline.
    for _ in 0..8 {
        match stderr.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                let keep = count.min(REASON_MAX as usize - text.len());
                text.extend_from_slice(&buffer[..keep]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::OpenOptionsExt;

    #[test]
    fn audit_regression_rule_timeout_closes_the_helpers_descriptor() {
        let root = std::env::temp_dir().join(format!("swarm-rule-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let pipe = root.join("pipe");
        let ready = root.join("ready");

        assert!(
            std::process::Command::new("mkfifo")
                .arg(&pipe)
                .status()
                .unwrap()
                .success()
        );
        let mut reader = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&pipe)
            .unwrap();

        let command = [
            "sh".into(),
            "-c".into(),
            "exec 3>\"$1\"; sleep 1 & printf ready >\"$2\"; wait".into(),
            "rule".into(),
            pipe.to_string_lossy().into_owned(),
            ready.to_string_lossy().into_owned(),
        ];
        let result = run(&command, "", None, "", Duration::from_millis(300));
        let mut byte = [0];
        let deadline = Instant::now() + Duration::from_millis(100);

        let eof = loop {
            match reader.read(&mut byte) {
                Ok(0) => break true,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                _ => break false,
            }
        };

        let started = ready.exists();
        std::fs::remove_dir_all(root).unwrap();
        assert!(matches!(result, Outcome::Failed(_)));
        assert!(started && eof);
    }
}
