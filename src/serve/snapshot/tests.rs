//! Tests for server snapshot.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn transcript_and_snapshot_read_real_synthetic_store_without_marking_notices() {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    swarm::store::add_notice(
        &connection,
        Some(&fixture.session),
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
