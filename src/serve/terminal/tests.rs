//! Tests for the terminal socket.

use super::*;

#[test]
fn a_ticket_works_once_and_names_its_target() {
    let target = Target::File(PathBuf::from("/tmp/x.output"));
    let ticket = mint(target.clone()).unwrap();
    assert_eq!(ticket.len(), 64);
    assert_eq!(redeem(&ticket), Some(target));
    assert_eq!(redeem(&ticket), None);
    assert_eq!(redeem("guess"), None);
}

#[test]
fn an_output_file_must_be_a_claude_task_file_of_this_user() {
    use std::os::unix::fs::MetadataExt;
    let root = std::env::temp_dir();
    let uid = std::fs::metadata(&root).unwrap().uid();
    let dir = PathBuf::from(format!(
        "/tmp/claude-{uid}/swarm-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
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
    assert_eq!(output_file(&root, &dir.join("../../etc/hosts")), None);
    assert_eq!(output_file(&root, Path::new("/etc/hosts")), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The socket takes no bearer header, so its ticket, Host, and Origin are its whole guard.
#[tokio::test]
async fn the_socket_needs_a_fresh_ticket_the_host_and_the_app_origin() {
    use super::super::test_support::Fixture;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    let ticket = mint(Target::File(file)).unwrap();

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
    assert_eq!(status(open(&ticket, &host, Some(&origin))).await, "101");
    assert_eq!(status(open(&ticket, &host, Some(&origin))).await, "401");
    let post = format!(
        "POST /api/terminal/ticket HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"
    );
    assert_eq!(status(post).await, "401");
}

/// The shipped tmux-swarm `attach`, pointed at a private tmux server, in a PTY: keys reach the
/// pane, and closing the PTY ends the client and puts the window back at its size.
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
    tmux("new-session -d -s t -x 120 -y 40 cat");
    let pane = tmux("display -p -t t '#{pane_id}'");
    let root = std::env::temp_dir().join(format!("swarm-term-root-{}", std::process::id()));
    std::fs::create_dir_all(root.join("adapters")).unwrap();
    let line = swarm::adapter::shipped("tmux-swarm")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("attach = "))
        .unwrap()
        .replace("tmux -L swarm ", &format!("tmux -L {server} -f /dev/null "));
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
    let mut reader = pair.master.try_clone_reader().unwrap();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while matches!(reader.read(&mut buf), Ok(n) if n > 0) {}
    });
    drop(writer);
    drop(pair.master);
    hang_up(child);
    assert_eq!(
        tmux("display -p -t t '#{window_width}x#{window_height} #{session_attached}'"),
        "120x40 0"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(tmux("display -p -t t '#{pane_current_command}'"), "cat");
    tmux("kill-server");
    let _ = std::fs::remove_dir_all(&root);
}
