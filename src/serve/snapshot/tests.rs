//! Tests for server snapshot.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn snapshot_clock_is_sent_without_changing_the_tree_revision() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut fixture = Fixture::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    Arc::get_mut(&mut fixture.app).unwrap().port = addr.port();
    let app = fixture.app.clone();
    let start = discovery::now() - 65;
    app.hub.lock().unwrap().agents = json!({
        (fixture.session.clone()): [{"name": "worker", "state": "working", "turn_start_time_s": start}]
    });
    let fixture_app = app.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router(fixture_app)).await.unwrap();
    });

    refresh_tree(&app).unwrap();
    let revision = app.hub.lock().unwrap().version;
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "GET /api/snapshot HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        app.token
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    server.abort();

    let (headers, body) = reply.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("HTTP/1.1 200"), "{headers}");
    let report: Value = serde_json::from_str(body).unwrap();
    let agent = &report["agents"][&fixture.session][0];
    assert_eq!(agent["turn_start_time_s"], start);
    assert!(agent["turn_clock_time_s"].as_i64().unwrap() >= start + 65);

    // The unmodified tree must remain equal after the wall clock advances to the next second.
    tokio::time::sleep(Duration::from_secs(1)).await;
    refresh_tree(&app).unwrap();
    assert_eq!(app.hub.lock().unwrap().version, revision);
}

#[tokio::test]
async fn transcript_and_snapshot_read_real_synthetic_store_without_marking_notices() {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    swarm::store::add_notice(
        &connection,
        Some(&fixture.session),
        None,
        None,
        "synthetic",
        "notice",
    )
    .unwrap();
    let Json(snapshot) = snapshot(State(fixture.app.clone())).await.unwrap();
    assert_eq!(snapshot["sessions"][0]["id"], fixture.session);
    assert!(snapshot["sessions"][0]["project"].is_string());
    assert_eq!(
        snapshot["sessions"][0]["workspace"],
        env!("CARGO_MANIFEST_DIR")
    );
    let unmarked: i64 = connection
        .query_row(
            "SELECT count(*) FROM notice WHERE posted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unmarked, 1);
    let Json(page) = transcript(
        State(fixture.app.clone()),
        Query(TranscriptQuery {
            session: fixture.session.clone(),
            agent: "chair".into(),
            tail: 2000,
            before: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(page["page"]["start_offset"], 0);
    assert_eq!(
        page["page"]["end_offset"],
        std::fs::metadata(&fixture.log).unwrap().len()
    );
    assert!(page["events"].as_array().unwrap().len() > 5);
    assert_eq!(
        resolve_log(&fixture.app.root, &fixture.session, "missing")
            .err()
            .unwrap()
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        resolve_log(&fixture.app.root, &fixture.session, "worker")
            .err()
            .unwrap()
            .0,
        StatusCode::NOT_FOUND
    );
    connection
        .execute(
            "UPDATE session SET archived_at = unixepoch() WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    assert!(resolve_log(&fixture.app.root, &fixture.session, "chair").is_ok());
}
