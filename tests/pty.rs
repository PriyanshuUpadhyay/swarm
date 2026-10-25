// `swarm pty` runs a command on its own terminal and answers the OSC 7501 probe (ADR 0122).

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn pty(script: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["pty", "--", "sh", "-c", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!(
                "swarm pty did not exit: {:?}",
                child.wait_with_output().unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn answers_the_probe_on_the_command_terminal() {
    // The probe reply has no newline, so the command reads its terminal raw; the reply is 9 bytes.
    let output = pty(
        r"stty raw -echo; printf '\033]7501;?\033\\'; dd bs=1 count=9 2>/dev/null | od -An -tx1",
    );
    let text = String::from_utf8_lossy(&output.stdout);
    // `od` pads its columns differently on macOS and Linux.
    let hex: String = text.split_whitespace().collect();
    assert!(
        hex.contains("1b5d373530313b3f07"),
        "{text:?} {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn passes_output_and_exit_code_through() {
    let output = pty("echo hello; exit 7");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hello"),
        "{output:?}"
    );
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn a_command_that_cannot_start_exits_127() {
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["pty", "--", "/nonexistent/swarm-test-cli"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(127), "{output:?}");
}
