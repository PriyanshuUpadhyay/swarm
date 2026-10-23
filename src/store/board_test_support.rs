//! Shared fixtures for the board tests.

use super::tests::*;
use super::*;

pub(super) const PROJECT: &str = "repository:/hub/.bare";

pub(super) const NOW: i64 = 1_000_000;

pub(super) const SRC: &str = "src/a.rs@3f2a1c9";

pub(super) fn fixture(tag: &str) -> (Connection, std::path::PathBuf) {
    (seed(), temp_root(&format!("board-{tag}")))
}

pub(super) fn put(
    connection: &mut Connection,
    root: &Path,
    author: &str,
    new: NewPost,
) -> Result<Posted, String> {
    post(connection, root, PROJECT, SESSION, author, NOW, &new).map_err(|e| e.to_string())
}

pub(super) fn n(posted: Result<Posted, String>) -> i64 {
    match posted.unwrap() {
        Posted::New(n, _) | Posted::Again(n) => n,
        other => panic!("not a post: {other:?}"),
    }
}

pub(super) fn finding(subject: &str) -> NewPost<'_> {
    NewPost {
        kind: "finding",
        subject,
        sources: vec![SRC.into()],
        ..Default::default()
    }
}

pub(super) fn refused(result: Result<Posted, String>, start: &str) {
    let error = result.unwrap_err();
    assert!(error.starts_with(&format!("swarm: {start}")), "{error}");
}

pub(super) fn claim(work: &str, generation: Option<i64>) -> NewPost<'_> {
    NewPost {
        kind: "claim",
        work_key: Some(work),
        lease_secs: Some(60),
        generation,
        subject: "mine",
        ..Default::default()
    }
}

pub(super) fn handoff(work: &str, generation: i64) -> NewPost<'_> {
    NewPost {
        kind: "handoff",
        work_key: Some(work),
        generation: Some(generation),
        subject: "done",
        ..Default::default()
    }
}

/// The board messages as (session, recipient, kind, body).
pub(super) fn rings(c: &Connection, root: &Path) -> Vec<(String, String, String, String)> {
    let mut statement = c
        .prepare("SELECT session_id, recipient_id, kind, body_path FROM message WHERE sender_id = 'board' ORDER BY session_id, seq")
        .unwrap();
    let rows = statement
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?))
        })
        .unwrap();
    rows.map(|r| {
        let (s, to, kind, path) = r.unwrap();
        (
            s,
            to,
            kind,
            std::fs::read_to_string(root.join(path)).unwrap(),
        )
    })
    .collect()
}

pub(super) fn ring(
    session: &str,
    to: &str,
    kind: &str,
    body: &str,
) -> (String, String, String, String) {
    (session.into(), to.into(), kind.into(), body.into())
}
