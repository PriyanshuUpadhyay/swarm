// `swarm pty --host` keeps a chair's children on PTYs it owns, and `swarm pty <verb>` drives them
// (ADR 0126).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Host {
    home: PathBuf,
    session: String,
    chair: Child,
}

impl Host {
    /// A host whose chair runs until `stop` exists. The home is long, as a branch build's is.
    fn start() -> Host {
        let home = std::env::temp_dir().join(format!(
            "swarm-pty-host-test-with-a-long-name-{}",
            uuid::Uuid::now_v7().simple()
        ));
        std::fs::create_dir_all(&home).unwrap();
        let session = uuid::Uuid::now_v7().to_string();
        let chair = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .args(["pty", "--host", "--", "sh", "-c"])
            .arg(format!(
                "echo \"$SWARM_PTY_HOST\" > {0}/host; while [ ! -e {0}/stop ]; do sleep 0.1; done",
                home.display()
            ))
            .env("SWARM_HOME", &home)
            .env("SWARM_SESSION_ID", &session)
            .env("SHELL", "/bin/sh")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let host = Host {
            home,
            session,
            chair,
        };
        wait_for(
            || read(&host.home.join("host")).ends_with('\n'),
            "the chair's SWARM_PTY_HOST",
        );
        host
    }

    fn socket(&self) -> PathBuf {
        PathBuf::from(read(&self.home.join("host")).trim())
    }

    /// `swarm pty <verb>` with the vars the adapter runtime exports.
    fn verb(&self, verb: &str, vars: &[(&str, &str)]) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
        command
            .args(["pty", verb])
            .env("SWARM_HOME", &self.home)
            .env("SWARM_SESSION_ID", &self.session)
            .env("SWARM_PTY_HOST", self.socket())
            .current_dir(&self.home);
        for (key, value) in vars {
            command.env(format!("SWARM_{}", key.to_uppercase()), value);
        }
        command.output().unwrap()
    }

    fn out(&self, verb: &str, vars: &[(&str, &str)]) -> String {
        let output = self.verb(verb, vars);
        assert!(output.status.success(), "{verb}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    /// A child whose output is copied to `feed` in the home.
    fn spawn(&self) -> String {
        let feed = format!("cat > {}/feed", self.home.display());
        self.out("spawn", &[("agent_id", "w1"), ("feed", &feed)])
    }

    fn stop(&mut self) {
        std::fs::write(self.home.join("stop"), "").unwrap();
        wait_for(
            || self.chair.try_wait().unwrap().is_some(),
            "the chair to exit",
        );
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.chair.kill();
        let _ = self.chair.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn wait_for(mut done: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn a_hosted_child_takes_rings_and_keys_and_shows_its_screen() {
    let host = Host::start();
    let pane = host.spawn();
    assert!(
        pane.starts_with(&format!("pty:{}:", host.session)),
        "{pane}"
    );
    let pane = pane.as_str();
    assert!(host.out("list", &[]).contains(&format!("{pane} ")));
    host.out("ring", &[("pane", pane), ("text", "echo hosted-$((40+2))")]);
    wait_for(
        || host.out("screen", &[("pane", pane)]).contains("hosted-42"),
        "the ring on the screen",
    );
    assert!(host.out("capture", &[("pane", pane)]).contains("hosted-42"));
    assert!(read(&host.home.join("feed")).contains("hosted-42"));
    // A key name becomes its bytes: the digit, then Enter as CR.
    host.out("ring", &[("pane", pane), ("text", "read x; echo got-$x")]);
    host.out("key", &[("pane", pane), ("key", "7")]);
    host.out("key", &[("pane", pane), ("key", "Enter")]);
    wait_for(
        || host.out("screen", &[("pane", pane)]).contains("got-7"),
        "the key's answer",
    );
    host.out("close", &[("pane", pane)]);
    wait_for(
        || !host.out("list", &[]).contains(&format!("{pane} ")),
        "the pane to leave the list",
    );
    let closed = host.verb("screen", &[("pane", pane)]);
    assert!(!closed.status.success());
}

#[test]
fn the_host_answers_a_cursor_report() {
    let host = Host::start();
    let pane = host.spawn();
    let reply = host.home.join("reply");
    // Raw, so the reply needs no newline; the cursor stands on row 2, column 3 when it asks.
    let line = format!(
        "stty raw -echo; printf '\\033[2;3H\\033[6n'; dd bs=1 count=6 2>/dev/null > {}; stty sane",
        reply.display()
    );
    host.out("ring", &[("pane", &pane), ("text", &line)]);
    wait_for(|| read(&reply).len() == 6, "the cursor report");
    assert_eq!(read(&reply), "\x1b[2;3R");
}

// Swarm ships for macOS, whose /bin/sh is bash. Linux's dash does not pass a hangup on to its jobs.
#[cfg(target_os = "macos")]
#[test]
fn children_and_their_jobs_stop_when_the_chair_exits() {
    let mut host = Host::start();
    let pane = host.spawn();
    let pids = host.home.join("pids");
    // A background job and a foreground job, as a CLI and a process it left behind.
    let line = format!("sleep 1000 & echo $$ $! > {}; sleep 999", pids.display());
    host.out("ring", &[("pane", &pane), ("text", &line)]);
    wait_for(|| read(&pids).ends_with('\n'), "the child's pids");
    let pids = read(&pids);
    let pids: Vec<&str> = pids.split_whitespace().collect();
    assert!(pids.iter().all(|pid| alive(pid)));
    host.stop();
    assert!(!host.socket().exists());
    wait_for(
        || pids.iter().all(|pid| !alive(pid)),
        "the child to hang up",
    );
}

#[test]
fn children_stop_when_the_host_is_killed() {
    let host = Host::start();
    let pane = host.spawn();
    let pids = host.home.join("pids");
    let line = format!("echo $$ > {}; sleep 999", pids.display());
    host.out("ring", &[("pane", &pane), ("text", &line)]);
    wait_for(|| read(&pids).ends_with('\n'), "the child's pid");
    let pid = read(&pids).trim().to_string();
    // SIGKILL runs no cleanup; the kernel hangs the child up when the master closes.
    let shim = host.chair.id().to_string();
    Command::new("kill").args(["-9", &shim]).status().unwrap();
    wait_for(|| !alive(&pid), "the child to hang up");
}

#[test]
fn a_second_chair_of_the_session_leaves_the_live_host_alone() {
    let mut host = Host::start();
    let pane = host.spawn();
    let second = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "pty",
            "--host",
            "--",
            "sh",
            "-c",
            "printf %s \"$SWARM_PTY_HOST\"",
        ])
        .env("SWARM_HOME", &host.home)
        .env("SWARM_SESSION_ID", &host.session)
        .env("SWARM_PTY_HOST", host.socket())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(second.status.success(), "{second:?}");
    // It runs with no host, and drops the inherited one, so its children would open tmux panes.
    assert_eq!(String::from_utf8_lossy(&second.stdout), "");
    assert!(String::from_utf8_lossy(&second.stderr).contains("already serves this session"));
    // The first chair's child is still reachable, after the second chair has exited.
    host.out("screen", &[("pane", &pane)]);
    host.stop();
}

#[test]
fn a_client_that_sends_a_byte_at_a_time_gets_one_deadline() {
    use std::io::{Read, Write};
    let mut host = Host::start();
    let mut stream = std::os::unix::net::UnixStream::connect(host.socket()).unwrap();
    let started = Instant::now();
    // Each byte comes well inside the 5 s deadline, so only a deadline for the whole line ends it.
    let mut reply = String::new();
    while started.elapsed() < Duration::from_secs(8) {
        if stream.write_all(b" ").is_err() {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
        stream.set_nonblocking(true).unwrap();
        let mut chunk = [0; 4096];
        if let Ok(read) = stream.read(&mut chunk) {
            reply.push_str(&String::from_utf8_lossy(&chunk[..read]));
        }
        stream.set_nonblocking(false).unwrap();
        if reply.contains('\n') {
            break;
        }
    }
    assert!(reply.contains("\"ok\":false"), "{reply:?}");
    assert!(
        started.elapsed() < Duration::from_secs(7),
        "{:?}",
        started.elapsed()
    );
    host.stop();
}

#[test]
fn a_child_that_reads_no_input_fails_a_ring_and_still_shows_its_output() {
    use std::io::{BufRead, Write};
    let host = Host::start();
    let pane = host.spawn();
    let n: u64 = pane.rsplit_once(':').unwrap().1.parse().unwrap();
    let ready = host.home.join("ready");
    // Raw, so a full input queue stops the writer. Then it asks for the cursor many times while
    // nothing reads its input, and the host must go on reading what it prints.
    let line = format!(
        "stty raw -echo; touch {}; sleep 2; i=0; while [ $i -lt 300 ]; do printf '\\033[6nline%s\\r\\n' $i; i=$((i+1)); done; echo DO''NE; sleep 999",
        ready.display()
    );
    host.out("ring", &[("pane", &pane), ("text", &line)]);
    wait_for(|| ready.exists(), "the child in raw mode");
    let write = |text: &str| {
        let stream = std::os::unix::net::UnixStream::connect(host.socket()).unwrap();
        let request = serde_json::json!({"op": "write", "n": n, "text": text});
        writeln!(&stream, "{request}").unwrap();
        let mut reply = String::new();
        std::io::BufReader::new(&stream)
            .read_line(&mut reply)
            .unwrap();
        reply
    };
    let text = "x".repeat(1000);
    let full = (0..200)
        .map(|_| write(&text))
        .find(|reply| reply.contains("\"ok\":false"));
    assert!(full.is_some_and(|reply| reply.contains("reads no input")));
    wait_for(
        || host.out("screen", &[("pane", &pane)]).contains("DONE"),
        "the output after the queries",
    );
}

#[test]
fn a_viewer_sees_the_screen_types_keys_and_the_last_one_out_restores_the_size() {
    let host = Host::start();
    let pane = host.spawn();
    host.out("ring", &[("pane", &pane), ("text", "echo before-$((1+1))")]);
    wait_for(
        || host.out("screen", &[("pane", &pane)]).contains("before-2"),
        "the first line",
    );
    let mut viewer = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["pty", "attach"])
        .env("SWARM_HOME", &host.home)
        .env("SWARM_PANE", &pane)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = viewer.stdout.take().unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let reader = {
        let seen = seen.clone();
        std::thread::spawn(move || {
            let mut chunk = [0; 4096];
            while let Ok(read @ 1..) = std::io::Read::read(&mut out, &mut chunk) {
                seen.lock().unwrap().extend_from_slice(&chunk[..read]);
            }
        })
    };
    let text = || String::from_utf8_lossy(&seen.lock().unwrap()).into_owned();
    // The screen comes first, then the keys typed here reach the shell and its output comes back.
    wait_for(|| text().contains("before-2"), "the screen");
    let mut keys = viewer.stdin.take().unwrap();
    std::io::Write::write_all(&mut keys, b"echo typed-$((2+3))\r").unwrap();
    wait_for(|| text().contains("typed-5"), "the typed line's output");
    host.out("ring", &[("pane", &pane), ("text", "stty size")]);
    wait_for(|| text().contains("100 80"), "the default size");
    // With no terminal the viewer asks for no size; the resize op that its SIGWINCH sends sets one.
    let n: u64 = pane.rsplit(':').next().unwrap().parse().unwrap();
    let stream = std::os::unix::net::UnixStream::connect(host.socket()).unwrap();
    let request = format!("{{\"op\":\"resize\",\"n\":{n},\"rows\":30,\"cols\":100}}\n");
    std::io::Write::write_all(&mut &stream, request.as_bytes()).unwrap();
    let mut reply = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(&stream), &mut reply).unwrap();
    assert!(reply.contains("\"ok\":true"), "{reply}");
    host.out("ring", &[("pane", &pane), ("text", "stty size")]);
    wait_for(|| text().contains("30 100"), "the new size");
    // The viewer leaves when its input ends, and the host closes its stream.
    drop(keys);
    wait_for(
        || viewer.try_wait().unwrap().is_some(),
        "the viewer to leave",
    );
    reader.join().unwrap();
    assert!(host.out("list", &[]).contains(&format!("{pane} ")));
    // The last viewer out puts the pane back at its detached size.
    host.out("ring", &[("pane", &pane), ("text", "stty size > size")]);
    wait_for(
        || read(&host.home.join("size")) == "100 80\n",
        "the size put back",
    );
}

#[test]
fn a_viewer_gets_no_query_the_host_answered_and_ends_with_its_pane() {
    let host = Host::start();
    let pane = host.spawn();
    let mut viewer = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["pty", "attach"])
        .env("SWARM_HOME", &host.home)
        .env("SWARM_PANE", &pane)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = viewer.stdout.take().unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let reader = {
        let seen = seen.clone();
        std::thread::spawn(move || {
            let mut chunk = [0; 4096];
            while let Ok(read @ 1..) = std::io::Read::read(&mut out, &mut chunk) {
                seen.lock().unwrap().extend_from_slice(&chunk[..read]);
            }
        })
    };
    let text = || String::from_utf8_lossy(&seen.lock().unwrap()).into_owned();
    // A viewer's terminal would answer a cursor query again, into the pane.
    host.out(
        "ring",
        &[
            ("pane", &pane),
            ("text", "printf 'q\\033[6nr'; echo done-$((1+1))"),
        ],
    );
    wait_for(|| text().contains("done-2"), "the query's line");
    assert!(text().contains("qr"), "{:?}", text());
    assert!(!text().contains("\x1b[6n"), "{:?}", text());
    // The viewer's input stays open, so only the pane's end can end it.
    let _keys = viewer.stdin.take().unwrap();
    // The host's own answer sits in the shell's input line, so C-u clears it first.
    host.out("key", &[("pane", &pane), ("key", "C-u")]);
    host.out("ring", &[("pane", &pane), ("text", "exit")]);
    wait_for(
        || !host.out("list", &[]).contains(&format!("{pane} ")),
        "the pane to end",
    );
    wait_for(
        || viewer.try_wait().unwrap().is_some(),
        "the viewer to end with its pane",
    );
    reader.join().unwrap();
}

/// One request on the host socket, and its reply line.
fn ask(socket: &Path, request: serde_json::Value) -> serde_json::Value {
    use std::io::{BufRead, Write};
    let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
    writeln!(&stream, "{request}").unwrap();
    let mut line = String::new();
    std::io::BufReader::new(&stream)
        .read_line(&mut line)
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

/// A headless host process, killed on drop, so a failed assert leaves no host behind.
struct Headless(Child);

impl Drop for Headless {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A headless host, as `launch orchestrator` starts it, with no TERM in its env, as an app's
/// serve may have none.
fn headless(home: &Path) -> (Headless, PathBuf) {
    use std::io::BufRead;
    let mut host = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(["pty", "host"])
        .env("SWARM_HOME", home)
        .env("SWARM_SESSION_ID", uuid::Uuid::now_v7().to_string())
        .env("SHELL", "/bin/sh")
        .env_remove("TERM")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(host.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    (Headless(host), PathBuf::from(line.trim()))
}

#[test]
fn a_headless_host_takes_its_first_live_pane_as_chair_after_a_failed_spawn() {
    let home = std::env::temp_dir().join(format!(
        "swarm-pty-headless-{}",
        uuid::Uuid::now_v7().simple()
    ));
    std::fs::create_dir_all(&home).unwrap();
    let (mut host, socket) = headless(&home);
    let spawn = |cwd: &Path| {
        let request =
            serde_json::json!({"op": "spawn", "cwd": cwd, "env": {}, "feed": "cat > /dev/null"});
        ask(&socket, request)
    };
    // A launch whose spawn fails uses up a pane number; the launch after it starts the chair.
    assert_eq!(spawn(&home.join("gone"))["ok"], false);
    let chair = spawn(&home);
    let n: u64 = chair["out"]
        .as_str()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    wait_for(
        || ask(&socket, serde_json::json!({"op": "screen", "n": n}))["ok"] == true,
        "the chair",
    );
    // The host looks for its chair every 200 ms, and a real chair runs far longer.
    std::thread::sleep(Duration::from_secs(1));
    ask(&socket, serde_json::json!({"op": "close", "n": n}));
    wait_for(
        || host.0.try_wait().unwrap().is_some(),
        "the host to end with its chair",
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_pane_gets_a_terminal_type_when_its_host_has_none() {
    let home =
        std::env::temp_dir().join(format!("swarm-pty-term-{}", uuid::Uuid::now_v7().simple()));
    std::fs::create_dir_all(&home).unwrap();
    let (host, socket) = headless(&home);
    let request =
        serde_json::json!({"op": "spawn", "cwd": home, "env": {}, "feed": "cat > /dev/null"});
    let pane = ask(&socket, request);
    let n: u64 = pane["out"]
        .as_str()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let file = home.join("term");
    let text = format!("echo \"[$TERM]\" > {}\n", file.display());
    assert_eq!(
        ask(
            &socket,
            serde_json::json!({"op": "write", "n": n, "text": text})
        )["ok"],
        true
    );
    wait_for(|| read(&file).ends_with('\n'), "the pane's TERM");
    let term = read(&file);
    ask(&socket, serde_json::json!({"op": "close", "n": n}));
    drop(host);
    let _ = std::fs::remove_dir_all(&home);
    // tmux gives its panes a terminal type too, whatever its server's env holds.
    assert_eq!(term, "[xterm-256color]\n");
}
