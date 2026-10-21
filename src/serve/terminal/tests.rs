//! Tests for the terminal socket.

use super::*;

/// The terminal slots are one global count, so the test that fills them and the test that opens a
/// socket take turns.
static SLOTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A fresh dir under `/tmp/claude-<uid>`, with the server root whose owner sets that uid.
fn task_dir(name: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::MetadataExt;
    let root = std::env::temp_dir();
    let uid = std::fs::metadata(&root).unwrap().uid();
    let dir = PathBuf::from(format!("/tmp/claude-{uid}/{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    (root, dir)
}

#[test]
fn a_ticket_works_once_and_names_its_target() {
    let target = Target::File(PathBuf::from("/tmp/x.output"));
    let ticket = mint(target.clone()).unwrap().unwrap();
    assert_eq!(ticket.len(), 64);
    assert_eq!(redeem(&ticket), Some(target));
    assert_eq!(redeem(&ticket), None);
    assert_eq!(redeem("guess"), None);
}

#[test]
fn an_output_file_must_be_a_claude_task_file_of_this_user() {
    let (root, dir) = task_dir("swarm-test");
    let good = dir.join("b1.output");
    std::fs::write(&good, "x").unwrap();
    let other = dir.join("notes.txt");
    std::fs::write(&other, "x").unwrap();
    let link = dir.join("link.output");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink("/etc/hosts", &link).unwrap();

    assert_eq!(
        output_file(&root, &good),
        Some(good.canonicalize().unwrap())
    );
    assert_eq!(output_file(&root, &other), None);
    assert_eq!(output_file(&root, &link), None);
    let folder = dir.join("d.output");
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(output_file(&root, &folder), None);
    assert_eq!(output_file(&root, &dir.join("../../etc/hosts")), None);
    assert_eq!(output_file(&root, Path::new("/etc/hosts")), None);
    // A task file of this user, but outside `/tmp/claude-<uid>`.
    let outside = PathBuf::from(format!("/tmp/swarm-test-{}.output", std::process::id()));
    std::fs::write(&outside, "x").unwrap();
    assert_eq!(output_file(&root, &outside), None);
    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The view keeps reading the file it opened. Swapping the path for a link to another file
/// after the open shows the real file's growth, never the other file.
#[test]
fn an_open_output_file_ignores_a_later_swap_of_its_path() {
    use std::io::Write;
    let (root, dir) = task_dir("swarm-swap");
    let path = dir.join("b1.output");
    std::fs::write(&path, "one\n").unwrap();
    let secret = dir.join("secret.txt");
    std::fs::write(&secret, "secret\n").unwrap();
    let mut shell = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();

    let mut file = open_output(&root, &path).unwrap();
    let mut at = 0;
    assert_eq!(grown(&mut file, &mut at).unwrap(), b"one\n");
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&secret, &path).unwrap();
    shell.write_all(b"two\n").unwrap();
    assert_eq!(grown(&mut file, &mut at).unwrap(), b"two\n");
    assert_eq!(grown(&mut file, &mut at).unwrap(), b"");
    assert!(open_output(&root, &path).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A long file's first view starts at a line, never inside a character or an escape sequence.
#[test]
fn the_first_view_of_a_long_file_starts_at_a_line() {
    let path = std::env::temp_dir().join(format!("swarm-tail-{}.output", std::process::id()));
    let line = "é\x1b[1mbold\x1b[0m\n";
    std::fs::write(&path, line.repeat(FILE_TAIL as usize / line.len() + 10)).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    let mut at = tail_start(&mut file).unwrap();
    let first = grown(&mut file, &mut at).unwrap();
    assert!(first.starts_with(line.as_bytes()));
    assert_eq!(
        tail_start(&mut std::fs::File::open("/dev/null").unwrap()).unwrap(),
        0
    );
    let _ = std::fs::remove_file(&path);
}

/// A child that ignores the hangup is killed with its whole group, so a grandchild such as the
/// tmux client does not outlive it.
#[test]
fn hang_up_kills_the_whole_group_when_the_hangup_is_ignored() {
    let pair = native_pty_system().openpty(size(80, 24)).unwrap();
    let mut command = CommandBuilder::new("sh");
    command.args(["-c", "trap '' HUP; sleep 60 & echo $!; wait"]);
    let child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut seen = String::new();
    let mut buf = [0u8; 256];
    while !seen.contains('\n') {
        let n = reader.read(&mut buf).unwrap();
        seen.push_str(&String::from_utf8_lossy(&buf[..n]));
    }
    let grandchild: i32 = seen.trim().parse().unwrap();
    std::thread::spawn(move || while matches!(reader.read(&mut buf), Ok(n) if n > 0) {});
    hang_up(child);
    // SAFETY: signal 0 only asks whether the process exists.
    let alive = || unsafe { libc::kill(grandchild, 0) } == 0;
    let until = Instant::now() + Duration::from_secs(2);
    while alive() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!alive(), "grandchild {grandchild} still runs");
}

/// A FIFO put where the checked file was is refused at once; a plain open would wait for a writer.
#[test]
fn a_fifo_in_place_of_the_file_is_refused_without_waiting() {
    let fifo = std::env::temp_dir().join(format!("swarm-fifo-{}.output", std::process::id()));
    let _ = std::fs::remove_file(&fifo);
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let (done, finished) = std::sync::mpsc::channel();
    let path = fifo.clone();
    std::thread::spawn(move || done.send(open_regular(&path).is_none()).unwrap());
    let refused = finished.recv_timeout(Duration::from_secs(2));
    let _ = std::fs::remove_file(&fifo);
    assert_eq!(refused, Ok(true));
}

/// Every other check passes: the file is under `/tmp/claude-0`, which this user made, and ends in
/// `.output`. Only its owner is wrong for a server whose root uid 0 owns.
#[test]
fn an_output_file_of_another_user_is_refused() {
    let base = Path::new("/tmp/claude-0");
    let made = !base.exists() && std::fs::create_dir(base).is_ok();
    let file = base.join(format!("swarm-test-{}.output", std::process::id()));
    if std::fs::write(&file, "x").is_err() {
        eprintln!("skipped: cannot write under {}", base.display());
        return;
    }
    use std::os::unix::fs::MetadataExt;
    if std::fs::metadata(&file).unwrap().uid() == 0 {
        eprintln!("skipped: run as root, so uid 0 owns the file");
        let _ = std::fs::remove_file(&file);
        return;
    }
    assert_eq!(output_file(Path::new("/"), &file), None);
    let _ = std::fs::remove_file(&file);
    if made {
        let _ = std::fs::remove_dir(base);
    }
}

/// A slot goes back when it drops, which is also what happens when axum drops an upgrade
/// callback unrun, so a failed upgrade cannot use up the terminals.
#[test]
fn a_dropped_slot_frees_its_place_and_a_full_server_says_when_to_retry() {
    let _turn = SLOTS.blocking_lock();
    let mut slots = Vec::new();
    while let Some(slot) = Slot::take() {
        slots.push(slot);
    }
    assert!(!slots.is_empty() && slots.len() <= MAX_LIVE);
    slots.pop();
    assert!(Slot::take().is_some());
    let busy = busy("too many terminals", RETRY_TERMINAL);
    assert_eq!(busy.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(busy.headers()[header::RETRY_AFTER], "5");
}

/// The socket takes no bearer header, so its ticket, Host, and Origin are its whole guard.
#[tokio::test]
async fn the_socket_needs_a_fresh_ticket_the_host_and_the_app_origin() {
    use super::super::test_support::Fixture;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let _turn = SLOTS.lock().await;
    let mut fixture = Fixture::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    Arc::get_mut(&mut fixture.app).unwrap().port = port;
    let app = fixture.app.clone();
    tokio::spawn(async move { axum::serve(listener, router(app)).await });
    let file = fixture.dir.join("b1.output");
    std::fs::write(&file, "out\n").unwrap();

    let status = |request: String| async move {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut head = [0u8; 12];
        stream.read_exact(&mut head).await.unwrap();
        String::from_utf8_lossy(&head[9..12]).to_string()
    };
    let origin = format!("http://127.0.0.1:{port}");
    let open = |ticket: &str, host: &str, origin: Option<&str>| {
        format!(
            "GET /api/terminal?ticket={ticket} HTTP/1.1\r\nHost: {host}\r\n{}Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: AAAAAAAAAAAAAAAAAAAAAA==\r\n\r\n",
            origin
                .map(|o| format!("Origin: {o}\r\n"))
                .unwrap_or_default()
        )
    };
    let host = format!("127.0.0.1:{port}");
    let ticket = mint(Target::File(file)).unwrap().unwrap();

    assert_eq!(status(open("guess", &host, Some(&origin))).await, "401");
    assert_eq!(status(open(&ticket, &host, None)).await, "403");
    assert_eq!(
        status(open(&ticket, &host, Some("http://evil.test"))).await,
        "403"
    );
    assert_eq!(
        status(open(&ticket, "evil.test", Some(&origin))).await,
        "403"
    );
    // A full server answers 429 and leaves the ticket for the retry.
    let mut slots = Vec::new();
    while let Some(slot) = Slot::take() {
        slots.push(slot);
    }
    assert_eq!(status(open(&ticket, &host, Some(&origin))).await, "429");
    drop(slots);
    assert_eq!(status(open(&ticket, &host, Some(&origin))).await, "101");
    assert_eq!(status(open(&ticket, &host, Some(&origin))).await, "401");
    let post = format!(
        "POST /api/terminal/ticket HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"
    );
    assert_eq!(status(post).await, "401");
}

/// The shipped tmux-swarm `attach`, pointed at a private tmux server, in a PTY: keys reach the
/// pane, and closing the PTY ends the client; the last viewer out puts the window back at its size.
#[test]
fn attach_in_a_pty_types_into_the_pane_and_restores_its_size() {
    if std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .is_err()
    {
        eprintln!("skipped: no tmux");
        return;
    }
    let server = format!("swarm-term-{}", std::process::id());
    let tmux = |args: &str| {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("tmux -L {server} -f /dev/null {args}"))
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    // Ends the private server also when an assert below fails.
    struct KillServer<'a>(&'a str);
    impl Drop for KillServer<'_> {
        fn drop(&mut self) {
            let _ = std::process::Command::new("tmux")
                .args(["-L", self.0, "kill-server"])
                .output();
        }
    }
    let _server = KillServer(&server);
    tmux("new-session -d -s t -x 120 -y 40 cat");
    let pane = tmux("display -p -t t '#{pane_id}'");
    // What a viewer killed with -9 leaves: its size on the window, the original in @swarm_size.
    tmux(
        "set-option -w -t t @swarm_size '120 40' \\; resize-window -t t -x 60 -y 15 \\; set-option -w -u -t t window-size",
    );
    let root = std::env::temp_dir().join(format!("swarm-term-root-{}", std::process::id()));
    std::fs::create_dir_all(root.join("adapters")).unwrap();
    let line = swarm::adapter::shipped("tmux-swarm")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("attach = "))
        .unwrap()
        .to_string();
    // Without this, a changed verb would run against the owner's real `swarm` server.
    assert!(
        line.contains("tmux -L swarm "),
        "attach verb changed: {line}"
    );
    let line = line.replace("tmux -L swarm ", &format!("tmux -L {server} -f /dev/null "));
    std::fs::write(root.join("adapters/tmux-swarm.conf"), format!("{line}\n")).unwrap();

    let pair = native_pty_system().openpty(size(50, 12)).unwrap();
    let child = pair
        .slave
        .spawn_command(attach_command(&root, "tmux-swarm", &pane).unwrap())
        .unwrap();
    drop(pair.slave);
    let mut writer = pty_writer(&*pair.master).unwrap();
    let wait_for = |what: &dyn Fn() -> bool| {
        let until = Instant::now() + Duration::from_secs(5);
        while !what() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        what()
    };
    assert!(wait_for(
        &|| tmux("display -p -t t '#{session_attached}'") == "1"
    ));
    assert_eq!(
        tmux("display -p -t t '#{window_width}x#{window_height}'"),
        "50x11"
    );
    writer.write_all(b"typed-through\r").unwrap();
    assert!(wait_for(
        &|| tmux("capture-pane -p -t t").contains("typed-through")
    ));

    // Like the server, a reader thread keeps a clone of the PTY open and drains it.
    let drain = |master: &dyn portable_pty::MasterPty| {
        let mut reader = master.try_clone_reader().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while matches!(reader.read(&mut buf), Ok(n) if n > 0) {}
        });
    };
    drain(&*pair.master);
    // A second viewer joins and stays after the first leaves; the size the last one out puts
    // back is still the one from before the first viewer.
    let second = native_pty_system().openpty(size(70, 20)).unwrap();
    let second_child = second
        .slave
        .spawn_command(attach_command(&root, "tmux-swarm", &pane).unwrap())
        .unwrap();
    drop(second.slave);
    drain(&*second.master);
    assert!(wait_for(
        &|| tmux("display -p -t t '#{session_attached}'") == "2"
    ));
    drop(writer);
    drop(pair.master);
    hang_up(child);
    assert_eq!(tmux("display -p -t t '#{session_attached}'"), "1");
    drop(second.master);
    hang_up(second_child);
    assert_eq!(
        tmux("display -p -t t '#{window_width}x#{window_height} #{session_attached}'"),
        "120x40 0"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(tmux("display -p -t t '#{pane_current_command}'"), "cat");
    let _ = std::fs::remove_dir_all(&root);
}
