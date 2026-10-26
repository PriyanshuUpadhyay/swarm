// A chair under `swarm pty --host` launches, drives, and closes a seat with no tmux (ADR 0126).

use super::*;
use std::time::{Duration, Instant};

fn wait_for(mut done: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_chair_under_a_host_launches_drives_and_closes_a_pty_seat() {
    let home = scratch("pty-host");
    // The seat reports working on its terminal at start and for each line it reads, as Claude
    // does under `swarm pty`. A probe of the CLI, with no model, ends at once.
    tool(
        &home,
        "claude",
        "case \"$*\" in *--model*) ;; *) exit 0 ;; esac\n\
         report() { printf '\\033]7501;state=working:app=claude\\007'; }\n\
         report; while read -r line; do report; done",
    );
    let app = [("SWARM_ADAPTER", "tmux-swarm")];
    let session = swarm(&home, &app, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let mut chair = Command::new(env!("CARGO_BIN_EXE_swarm"));
    chair
        .args(["pty", "--host", "--", "sh", "-c"])
        .arg("echo \"$SWARM_PTY_HOST\" > host; while [ ! -e stop ]; do sleep 0.1; done")
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .env("SWARM_SESSION_ID", &session)
        .current_dir(&home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    // A failed assert must not leave the chair holding the test's stderr open.
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut chair = Kill(chair.stderr(std::process::Stdio::null()).spawn().unwrap());
    let host = home.join("host");
    wait_for(
        || std::fs::read_to_string(&host).is_ok_and(|text| text.ends_with('\n')),
        "the host",
    );
    let socket = std::fs::read_to_string(&host).unwrap().trim().to_string();
    let env = [
        ("SWARM_ADAPTER", "tmux-swarm"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
        ("SWARM_PTY_HOST", socket.as_str()),
    ];
    let add = swarm(
        &home,
        &[("TMUX_PANE", "%9999"), env[0], env[1], env[2]],
        &["agent", "add", "orchestrator", "orchestrator"],
    );
    assert!(add.status.success(), "{}", stderr(&add));
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let launch = swarm(
        &home,
        &env,
        &[
            "launch",
            "seat",
            "chat",
            "--provider",
            "claude",
            "--model",
            "opus",
            "--cwd",
            &cwd.to_string_lossy(),
        ],
    );
    assert!(launch.status.success(), "{}", stderr(&launch));
    let pane = String::from_utf8(launch.stdout).unwrap().trim().to_string();
    assert!(pane.starts_with(&format!("pty:{session}:")), "{pane}");
    let tmux = Command::new("tmux")
        .args(["-L", "swarm", "has-session", "-t"])
        .arg(format!("=swarm_{session}_seat"))
        .stderr(std::process::Stdio::null())
        .status();
    assert!(
        tmux.is_err() || !tmux.unwrap().success(),
        "a tmux session exists"
    );

    // The seat's report reached the store through the host's feed.
    let seat = |field: &str| {
        let agents = swarm(&home, &env, &["agents", "--json"]);
        let agents: serde_json::Value = serde_json::from_slice(&agents.stdout).unwrap();
        agents["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == "seat")
            .map(|agent| agent[field].clone())
            .unwrap()
    };
    wait_for(|| seat("state") == "working", "the working report");
    assert_eq!(seat("alive"), true);

    let screen = || {
        let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .args(["pty", "screen"])
            .env("SWARM_HOME", &home)
            .env("SWARM_PANE", &pane)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let mut send = Command::new(env!("CARGO_BIN_EXE_swarm"));
    send.env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .envs(env)
        .args(["send", "seat", "ask"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut send = send.spawn().unwrap();
    std::io::Write::write_all(&mut send.stdin.take().unwrap(), b"hello seat").unwrap();
    let sent = send.wait_with_output().unwrap();
    assert!(sent.status.success(), "{}", stderr(&sent));
    // The terminal echoes the ring, and the seat's report for it proved the ring.
    wait_for(|| screen().contains("swarm: new message"), "the ring");
    assert!(!stderr(&sent).contains("unconfirmed"), "{}", stderr(&sent));
    for args in [&["interrupt", "seat"][..], &["key", "seat", "Up"]] {
        let output = swarm(&home, &env, args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }

    let close = swarm(&home, &env, &["close", "--now", "seat"]);
    assert!(close.status.success(), "{}", stderr(&close));
    assert_eq!(seat("pane"), serde_json::Value::Null);
    assert!(screen().is_empty());
    std::fs::write(home.join("stop"), "").unwrap();
    chair.0.wait().unwrap();
    let _ = std::fs::remove_dir_all(&home);
}
