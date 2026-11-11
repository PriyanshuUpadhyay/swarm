use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn temporary() -> PathBuf {
    let root = std::env::temp_dir().join(format!("swarm-pages-cli-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&root).unwrap();
    root
}

#[test]
fn pages_args_accept_only_the_declared_forms() {
    assert_eq!(parse(&[]).unwrap(), Command::List);
    assert_eq!(parse(&args(&["stop"])).unwrap(), Command::Stop);
    assert_eq!(
        parse(&args(&["--foreground"])).unwrap(),
        Command::Foreground(4748, Vec::new())
    );
    assert_eq!(
        parse(&args(&["--foreground", "--port", "9200"])).unwrap(),
        Command::Foreground(9200, Vec::new())
    );
    let id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        parse(&args(&["--foreground", "--id", &id, "--port", "9200"])).unwrap(),
        Command::Foreground(9200, vec![id])
    );
    assert_eq!(
        parse(&args(&["publish", "report"])).unwrap(),
        Command::Publish("report".into(), None)
    );
    assert_eq!(
        parse(&args(&["publish", "report", "--slug", "my-page"])).unwrap(),
        Command::Publish("report".into(), Some("my-page".into()))
    );
    for invalid in [
        vec!["stop", "extra"],
        vec!["--port", "9200"],
        vec!["--foreground", "--port", "0"],
        vec!["--foreground", "--port", "65536"],
        vec!["--foreground", "--id", "2026-10-21-page"],
        vec!["--foreground", "--id"],
        vec!["publish"],
        vec!["publish", "report", "--slug"],
    ] {
        assert!(parse(&args(&invalid)).is_err(), "{invalid:?}");
    }
}

#[test]
fn pages_default_slug_uses_the_folder_name() {
    assert_eq!(
        default_slug(Path::new("/tmp/ My REPORT_2! ")),
        "my-report-2"
    );
    assert_eq!(default_slug(Path::new("/tmp/Hello  World")), "hello--world");
    assert_eq!(default_slug(Path::new("/tmp/!!!")), "");
}

#[test]
fn pages_record_round_trips_and_is_private() {
    let root = temporary();
    let record = root.join("pages-url");
    write_record(&record, "http://127.0.0.1:4748/", 42).unwrap();
    assert_eq!(
        read_record(&record),
        Some(("http://127.0.0.1:4748/".into(), 42))
    );
    assert_eq!(
        fs::metadata(&record).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&record, fs::Permissions::from_mode(0o644)).unwrap();
    write_record(&record, "http://127.0.0.1:9200/", 43).unwrap();
    assert_eq!(
        read_record(&record),
        Some(("http://127.0.0.1:9200/".into(), 43))
    );
    assert_eq!(
        fs::metadata(&record).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pages_record_refuses_remote_urls_and_unsafe_pids() {
    let root = temporary();
    let record = root.join("pages-url");
    for invalid in [
        "http://evil.test:4748/\n42\n",
        "http://127.0.0.1:4748/#token=ab\n42\n",
        "http://127.0.0.1:0/\n42\n",
        "http://127.0.0.1:4748/\n-1\n",
        "http://127.0.0.1:4748/\n0\n",
        "http://127.0.0.1:4748/\n1\n",
        "http://127.0.0.1:4748/\n",
        "broken",
    ] {
        fs::write(&record, invalid).unwrap();
        assert_eq!(read_record(&record), None, "{invalid:?}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pages_publish_output_has_the_url_first_and_optional_commit_last() {
    let expected =
        "http://127.0.0.1:9200/2026-10-21-report/\nhash sha256:abcd\nsource /tmp/report\n";
    assert_eq!(
        publish_lines(
            "http://127.0.0.1:9200/",
            "2026-10-21-report",
            "abcd",
            Path::new("/tmp/report"),
            None
        ),
        expected
    );
    assert_eq!(
        publish_lines(
            "http://127.0.0.1:9200/",
            "2026-10-21-report",
            "abcd",
            Path::new("/tmp/report"),
            Some("commit 123abcd+dirty")
        ),
        format!("{expected}commit 123abcd+dirty\n")
    );
}

#[test]
fn pages_commit_line_marks_any_porcelain_change() {
    assert_eq!(commit_line("123abcd\n", ""), Some("commit 123abcd".into()));
    assert_eq!(
        commit_line("123abcd\n", "?? report/\n"),
        Some("commit 123abcd+dirty".into())
    );
    assert_eq!(
        commit_line("123abcd\n", " M index.html\n"),
        Some("commit 123abcd+dirty".into())
    );
    assert_eq!(commit_line("", ""), None);
    assert_eq!(commit_line("unexpected\noutput", ""), None);
}

fn probe_reply(response: &str) -> (bool, String) {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let response = response.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    let accepted = answers(
        &format!("http://127.0.0.1:{port}/"),
        std::process::id() as i32,
        Instant::now() + WAIT,
    );
    (accepted, server.join().unwrap())
}

#[tokio::test]
async fn pages_probe_checks_pid_against_stub_replies_and_the_real_router() {
    let pid = std::process::id();
    let wrong = format!("X-Swarm-Pages-Pid: {}\r\n", pid + 1);
    // Each stub reply is a row: the probe takes only the recorded pid, whatever the status or policy.
    let refused = [
        "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_owned(),
        format!(
            "HTTP/1.1 200 OK\r\nContent-Security-Policy: {}\r\n\r\n",
            pages_hub::CSP
        ),
        format!(
            "HTTP/1.1 200 OK\r\nContent-Security-Policy: {}\r\n{wrong}\r\n",
            pages_hub::CSP
        ),
    ];
    for reply in refused {
        assert!(
            !probe_reply(&reply).0,
            "a missing or wrong pid must be refused: {reply}"
        );
    }
    for status in ["200 OK", "404 Not Found"] {
        let (accepted, request) = probe_reply(&format!(
            "HTTP/1.1 {status}\r\nx-SwArM-pAgEs-PiD: {pid}\r\nContent-Length: 0\r\n\r\n"
        ));
        assert!(accepted, "the recorded pid must be accepted ({status})");
        assert!(request.starts_with("GET /.swarm-pages-probe HTTP/1.1\r\n"));
    }
    let (accepted, _) = probe_reply(&format!(
        "HTTP/1.1 404 Not Found\r\nX-Swarm-Pages-Pid: {pid}\r\nContent-Security-Policy: sandbox allow-scripts\r\n\r\n"
    ));
    assert!(accepted, "pid proof must survive a changed sandbox policy");

    let root = temporary();
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = pages_hub::router(root.clone(), port, None);
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::task::spawn_blocking(move || {
        let url = format!("http://127.0.0.1:{port}/");
        let pid = std::process::id() as i32;
        assert!(
            answers(&url, pid, Instant::now() + WAIT),
            "the real router must prove its own pid"
        );
        assert!(
            !answers(&url, pid + 1, Instant::now() + WAIT),
            "the real router must not prove another pid"
        );
    })
    .await
    .unwrap();
    server.abort();
    let _ = server.await;
    fs::remove_dir_all(root).unwrap();
}
