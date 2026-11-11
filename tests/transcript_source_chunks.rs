//! The public `SnapshotReader` fed ONE byte at a time (the CLI reads 8 KiB, so the other tests never
//! split a UTF-8 character or a CRLF pair). The fixture has literal emoji and CJK text, two complete
//! CRLF lines and an unterminated final line. The expected events and positions are the OLD native
//! values; see `tests/fixtures/transcript-worker/chunks/PROVENANCE.json`.
//!
//! Run with `--features serde_json/raw_value,serde_json/unbounded_depth`, like the worker test.

use std::path::PathBuf;

use serde_json::Value;
use swarm::view::transcript::source::{EndPolicy, SnapshotError, SnapshotReader, SourceMeta, Work};

/// A budget that never runs out. It stands in for the clock boundary, not for the domain.
struct Unlimited;

impl Work for Unlimited {
    fn check(&mut self) -> Result<(), SnapshotError> {
        Ok(())
    }
}

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/transcript-worker/chunks")
        .join(name);
    std::fs::read(path).expect("chunks fixture")
}

#[test]
fn one_byte_pushes_keep_utf8_crlf_and_the_final_fragment_policy() {
    let source = fixture("source.jsonl");
    let expected: Value = serde_json::from_slice(&fixture("expected.json")).expect("expected.json");
    for (name, end) in [
        ("snapshot_eof", EndPolicy::SnapshotEof),
        ("complete_lines", EndPolicy::CompleteLines),
    ] {
        let mut work = Unlimited;
        let mut reader = SnapshotReader::new(
            SourceMeta {
                source_id: "chunks".to_string(),
            },
            0,
            end,
        );
        for byte in &source {
            reader
                .push(std::slice::from_ref(byte), &mut work)
                .unwrap_or_else(|error| panic!("{name}: push failed: {error}"));
        }
        let snapshot = reader
            .finish(&mut work)
            .unwrap_or_else(|error| panic!("{name}: finish failed: {error}"));
        let actual = serde_json::to_value(&snapshot).expect("snapshot serializes");
        let want = &expected["policies"][name];
        assert_eq!(
            actual["source_id"], expected["source_id"],
            "{name}: source id"
        );
        assert_eq!(
            actual["read_end_byte"], want["read_end_byte"],
            "{name}: read end"
        );
        assert_eq!(
            actual["complete_end_byte"], want["complete_end_byte"],
            "{name}: complete end"
        );
        assert_eq!(actual["lines"], want["lines"], "{name}: lines");
    }
}
