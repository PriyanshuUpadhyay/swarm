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
    let deadline = std::time::Instant::now() + timeout;
    let child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let (bytes, status) = crate::profiles::native::read_stdout(child, u64::MAX, deadline);
    Some((
        status?.success(),
        String::from_utf8_lossy(&bytes.ok()?.ok()?).into_owned(),
    ))
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
