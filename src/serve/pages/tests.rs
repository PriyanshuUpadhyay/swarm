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
        Command::Foreground(4748)
    );
    assert_eq!(
        parse(&args(&["--foreground", "--port", "9200"])).unwrap(),
        Command::Foreground(9200)
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

#[test]
fn pages_list_shows_only_page_folders_newest_first() {
    let root = temporary();
    fs::create_dir(root.join("2026-10-20-old")).unwrap();
    fs::write(root.join("2026-10-20-old/index.html"), "old").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    fs::create_dir(root.join("2026-10-21-new")).unwrap();
    fs::write(root.join("2026-10-21-new/index.html"), "new").unwrap();
    fs::create_dir(root.join(".tmp-hidden")).unwrap();
    fs::create_dir(root.join("empty")).unwrap();
    fs::write(root.join("file"), "not a folder").unwrap();
    std::os::unix::fs::symlink(root.join("2026-10-21-new"), root.join("linked")).unwrap();
    assert_eq!(
        folders(&root).unwrap(),
        vec!["2026-10-21-new", "2026-10-20-old"]
    );
    assert!(folders(&root.join("missing")).unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
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
    let accepted = answers(&format!("http://127.0.0.1:{port}/"), Instant::now() + WAIT);
    (accepted, server.join().unwrap())
}

#[test]
fn pages_probe_refuses_http_200_without_shelf_csp() {
    let (accepted, _) = probe_reply("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    assert!(!accepted, "HTTP 200 without the shelf CSP must be refused");
}

#[test]
fn pages_probe_accepts_exact_shelf_csp_on_a_cheap_path() {
    const CSP: &str = "sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads";
    for status in ["200 OK", "404 Not Found"] {
        let (accepted, request) = probe_reply(&format!(
            "HTTP/1.1 {status}\r\nContent-Security-Policy: {CSP}\r\nContent-Length: 0\r\n\r\n"
        ));
        assert!(accepted, "the exact shelf CSP must be accepted ({status})");
        assert!(request.starts_with("GET /.swarm-pages-probe HTTP/1.1\r\n"));
    }
    let (accepted, _) =
        probe_reply("HTTP/1.1 200 OK\r\nContent-Security-Policy: sandbox allow-scripts\r\n\r\n");
    assert!(!accepted, "a different sandbox policy must be refused");
}
