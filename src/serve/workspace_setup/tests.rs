//! Tests for server workspace setup.

use super::*;
use crate::serve::test_support::*;

#[test]
fn setup_chunks_keep_utf8_boundaries_and_replace_invalid_bytes() {
    let mut pending = Vec::new();
    assert_eq!(setup_text(&mut pending, &[b'a', 0xf0, 0x9f], false), "a");
    assert_eq!(
        setup_text(&mut pending, &[0x98, 0x80, 0xff, 0xe2], false),
        "😀�"
    );
    assert_eq!(setup_text(&mut pending, &[], true), "�");
    assert!(pending.is_empty());
}

#[tokio::test]
async fn setup_streams_both_channels_reports_failure_and_kills_timeout_group() {
    let fixture =
        Fixture::at(std::env::temp_dir().join(format!("swarm-setup-{}", uuid::Uuid::now_v7())));
    let workspace = fixture.dir.to_str().unwrap();
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .insert("setup-test".into(), client(&[]));
    let result = run_setup(
        &fixture.app,
        workspace,
        "printf 'ready'; printf 'problem' >&2; exit 7",
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(result["state"], "failed");
    assert_eq!(result["exit_code"], 7);
    {
        let hub = fixture.app.hub.lock().unwrap();
        let frames: Vec<Value> = hub.streams["setup-test"]
            .queue
            .iter()
            .map(|f| serde_json::from_str(&f.data).unwrap())
            .collect();
        assert_eq!(frames.first().unwrap()["state"], "running");
        assert_eq!(frames.last().unwrap()["exit_code"], 7);
        assert!(
            frames
                .iter()
                .any(|f| f["channel"] == "stdout" && f["text"] == "ready")
        );
        assert!(
            frames
                .iter()
                .any(|f| f["channel"] == "stderr" && f["text"] == "problem")
        );
    }
    let result = run_setup(
        &fixture.app,
        workspace,
        "sleep 30 & printf '%s' \"$!\" > sleeper; wait",
        // The limit must outlast the shell's start on a loaded Mac, so its sleeper exists.
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(result["state"], "timeout");
    assert!(fixture.dir.is_dir());
    let pid = std::fs::read_to_string(fixture.dir.join("sleeper")).unwrap();
    until_dead(pid.trim()).await;
    let result = run_setup(
        &fixture.app,
        workspace,
        "sleep 30 & printf '%s' \"$!\" > detached; exit 0",
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(result["state"], "completed");
    let pid = std::fs::read_to_string(fixture.dir.join("detached")).unwrap();
    until_dead(pid.trim()).await;
}
