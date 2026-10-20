//! Rule process execution and bounded deny reasons.

use super::*;

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
    let mut process = std::process::Command::new(program);
    if let Some(cwd) = cwd.filter(|cwd| cwd.is_dir()) {
        process.current_dir(cwd);
    }
    let mut child = match process
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return Outcome::Failed(format!("{program} did not start: {error}")),
    };
    // Both pipes go through threads: a rule that does not read a large payload, or that writes
    // much to stderr, must not stall the runner past its deadline.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = input.to_string();
    std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (sender, reason) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = (&mut stderr).take(REASON_MAX).read_to_end(&mut text);
        // A character the read cut in two is dropped, else it would decode to a U+FFFD.
        if text.len() as u64 == REASON_MAX
            && let Some(chunk) = text.utf8_chunks().last()
        {
            text.truncate(text.len() - chunk.invalid().len());
        }
        // Each invalid byte decodes to a 3-byte U+FFFD, so the cut is made again on the text, at
        // a character boundary, to keep the reason within 64 KiB.
        let mut text = String::from_utf8_lossy(&text).into_owned();
        text.truncate(text.floor_char_boundary(REASON_MAX as usize));
        let _ = sender.send(text);
        // The rest is read and dropped, so a rule that writes more does not die on a closed pipe.
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < limit => std::thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::Failed(format!("no answer within {} s", limit.as_secs_f32()));
            }
            Err(error) => return Outcome::Failed(format!("cannot wait for it: {error}")),
        }
    };
    match status.code() {
        Some(0) => Outcome::Allow,
        Some(2) => Outcome::Deny(
            reason
                .recv_timeout(REASON_WAIT)
                .unwrap_or_default()
                .trim()
                .to_string(),
        ),
        Some(code) => Outcome::Failed(format!("exit {code}")),
        None => Outcome::Failed("killed by a signal".into()),
    }
}
