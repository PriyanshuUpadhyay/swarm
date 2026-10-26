// `swarm pty` runs a command on its own terminal and answers the OSC 7501 probe (ADR 0123).

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

#[test]
fn the_command_hangs_up_when_swarm_pty_is_killed() {
    use std::io::BufRead;
    // A pane close kills `swarm pty`; the command must not keep its terminal open and live on.
    let mut shim = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["pty", "--", "sh", "-c", "echo pid=$$; sleep 30"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(shim.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let pid = line.trim().strip_prefix("pid=").unwrap().to_string();
    shim.kill().unwrap();
    shim.wait().unwrap();
    let alive = || {
        Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let survived = alive();
    if survived {
        let _ = Command::new("kill").args(["-9", &pid]).status();
    }
    assert!(!survived, "pid {pid} outlived swarm pty");
}

#[test]
fn the_command_hangs_up_when_the_output_closes() {
    use std::io::{BufRead, Read};
    // With no reader of the output, the command must not block forever on a full terminal.
    let mut shim = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["pty", "--", "sh", "-c", "echo pid=$$; exec yes"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = std::io::BufReader::new(shim.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let pid = line.trim().strip_prefix("pid=").unwrap().to_string();
    output.read_exact(&mut [0; 100]).unwrap();
    drop(output);
    let deadline = Instant::now() + Duration::from_secs(5);
    while shim.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let exited = shim.try_wait().unwrap().is_some();
    if !exited {
        let _ = shim.kill();
        let _ = Command::new("kill").args(["-9", &pid]).status();
    }
    assert!(exited, "swarm pty kept running with no reader");
}
