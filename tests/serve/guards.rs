// Tests for guards.

use super::*;

#[test]
fn bearer_host_origin_and_body_guards_run_on_the_real_server() {
    let server = Server::new();
    let (status, headers, _) = server.request("GET", "/api/snapshot", "", "", false);
    assert_eq!(status, 401);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("www-authenticate: bearer realm=\"swarm\"")
    );
    assert!(!headers.contains("invalid_token"));
    let (status, headers, _) = server.request(
        "GET",
        "/api/snapshot",
        "Authorization: Bearer wrong\r\n",
        "",
        false,
    );
    assert_eq!(status, 401);
    assert!(headers.contains("error=\"invalid_token\""));
    let mut stream = server.connection();
    write!(stream, "GET /api/snapshot HTTP/1.1\r\nHost: evil.test\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n", server.token).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    assert_eq!(
        server
            .request(
                "GET",
                "/api/snapshot",
                "Origin: https://evil.test\r\n",
                "",
                true
            )
            .0,
        403
    );
    assert_eq!(
        server.request("POST", "/api/subscribe", "", "{}", true).0,
        415
    );
    let (status, headers, _) = server.request(
        "GET",
        "/api/snapshot",
        "Origin: http://127.0.0.1:5173\r\n",
        "",
        true,
    );
    assert_eq!(status, 200);
    assert!(!headers.to_ascii_lowercase().contains("access-control-"));
    let body = json!({"stream": "unknown", "keys": [], "padding": "x".repeat(65536)}).to_string();
    assert_eq!(
        server
            .request(
                "POST",
                "/api/subscribe",
                "Content-Type: application/json\r\n",
                &body,
                true
            )
            .0,
        413
    );
    assert_eq!(server.request("GET", "/api/unknown", "", "", false).0, 401);
}

#[test]
fn the_coordinator_leaves_notices_for_the_native_shell() {
    let server = Server::new();
    // Two coordinator passes run in this wait.
    std::thread::sleep(Duration::from_millis(2200));
    let connection = rusqlite::Connection::open(server.dir.join("home/.swarm/swarm.db")).unwrap();
    let unposted: i64 = connection
        .query_row(
            "SELECT count(*) FROM notice WHERE posted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unposted, 1);
}

/// The shell's Dock badge (ADR 0077) counts the chats that wait, and the owner can turn it off.
#[test]
fn badge_counts_waiting_chats_and_honours_the_badge_preference() {
    // Panes from the fixture adapter keep the agents alive; an ended agent never waits.
    let server = Server::configured(true, true);
    let badge = |want: u64| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let (status, _, body) = server.request("GET", "/api/badge", "", "", true);
            assert_eq!(status, 200, "{body}");
            let count = serde_json::from_str::<Value>(&body).unwrap()["count"].clone();
            if count == want || Instant::now() > deadline {
                assert_eq!(count, want);
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let root = server.dir.join("home/.swarm");
    let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
    swarm::store::set_state(
        &connection,
        &server.session,
        "worker",
        "waiting",
        "hook",
        None,
        None,
        1,
    )
    .unwrap();
    badge(1);
    let mut choices: Value =
        serde_json::from_str(&fs::read_to_string(root.join("choices.json")).unwrap()).unwrap();
    choices["prefs"] = json!({"notices": {"badge": false}});
    fs::write(root.join("choices.json"), choices.to_string()).unwrap();
    badge(0);
}
