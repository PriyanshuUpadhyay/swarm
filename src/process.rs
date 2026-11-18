// Child processes with a deadline.

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
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;

    // The read runs on its own thread, so a full pipe cannot stall the deadline loop.
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut stdout, &mut bytes).map(|_| bytes)
    });

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let bytes = reader.join().ok()?.ok()?;
                return Some((
                    status.success(),
                    String::from_utf8_lossy(&bytes).into_owned(),
                ));
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => return None,
        }
    }
}
