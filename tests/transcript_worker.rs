//! The private `swarm _transcript-worker --protocol 1` command (first migration unit).
//!
//! Every expected value comes from the OLD Zig translator and the OLD Rust row layer, captured before
//! this unit existed. `tests/fixtures/transcript-worker/PROVENANCE.json` lists the source and hash of
//! each file. Run with `--features serde_json/raw_value,serde_json/unbounded_depth` so the test can
//! decode the 255-level tool input that the worker replies with.

use serde_json::{Value, json};

#[path = "transcript_worker/client.rs"]
mod client;
#[path = "transcript_worker/fixtures.rs"]
mod fixtures;
#[path = "transcript_worker/numbers.rs"]
mod numbers;

use client::{MAX_FRAME_BYTES, MAX_PART_BYTES, WORK_MS, Worker};
use fixtures::{
    assert_same, decode_deep, expand_recipes, fixture_dir, load_expected, materialize_source,
};

#[test]
fn claude_snapshots_through_the_worker_match_the_old_translator_and_rows() {
    let expected = load_expected();
    for mut group in expected["groups"].as_array().expect("groups").clone() {
        expand_recipes(&mut group);
        let name = group["name"].as_str().expect("group name").to_string();
        let binding = json!({"source_id": name, "generation": 1});
        let mut worker = Worker::start();
        // A group with a `materialize` recipe builds its source in the worker's scratch dir.
        let file = match group.get("materialize") {
            Some(recipe) => {
                worker.snapshot_work_ms = group["work_ms"].as_u64().expect("work_ms");
                materialize_source(recipe, &worker.scratch)
            }
            None => group["file"].as_str().expect("file").to_string(),
        };
        let ready = worker.hello(1, WORK_MS);
        assert_eq!(
            ready,
            json!({"op": "ready", "protocol": 1, "formats": ["claude", "codex", "agy", "pi"]}),
            "{name}: handshake"
        );
        let opened = worker.open(1, &binding, "claude", &file);
        assert_eq!(
            opened,
            json!({"op": "opened", "request_id": 1, "binding": binding}),
            "{name}: open"
        );
        let mut request_id = 1;
        for snapshot in group["snapshots"].as_array().expect("snapshots") {
            request_id += 1;
            let label = format!("{name}/{}", snapshot["label"].as_str().expect("label"));
            let start = snapshot["start_byte"].as_u64().expect("start");
            let committed = worker
                .snapshot(
                    request_id,
                    &binding,
                    start,
                    &snapshot["end_byte"],
                    snapshot["end_policy"].as_str().expect("policy"),
                )
                .unwrap_or_else(|fault| {
                    panic!("{label}: the worker answered with a fault: {fault}")
                });
            let payload = &committed.value;
            let keys: Vec<&str> = payload
                .as_object()
                .expect("snapshot object")
                .keys()
                .map(String::as_str)
                .collect();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(
                sorted,
                [
                    "complete_end_byte",
                    "lines",
                    "read_end_byte",
                    "rows",
                    "source_id"
                ],
                "{label}: snapshot fields"
            );
            assert_eq!(
                payload["source_id"],
                json!(name),
                "{label}: the binding label is returned as is"
            );
            assert_eq!(
                payload["read_end_byte"], snapshot["read_end_byte"],
                "{label}: read end"
            );
            assert_eq!(
                payload["complete_end_byte"], snapshot["complete_end_byte"],
                "{label}: complete end"
            );
            // Lines carry the positions, the terminator flag and the OLD events in order.
            assert_same(&label, "lines", &payload["lines"], &snapshot["lines"]);
            // Rows are compared only where the OLD row layer was captured for the same input.
            if !snapshot["rows"].is_null() {
                assert_same(&label, "rows", &payload["rows"], &snapshot["rows"]);
            }
            if name == "giant-output" {
                assert!(
                    committed.payload_bytes > MAX_FRAME_BYTES && committed.parts > 16,
                    "{label}: the committed payload is larger than one frame"
                );
            }
            if name == "long-line" {
                assert!(
                    committed.payload_bytes > MAX_PART_BYTES && committed.parts >= 2,
                    "{label}: the reply spans parts"
                );
            }
        }
        request_id += 1;
        worker.close(request_id, &binding);
    }
    // The same CLI main path, for the two native numbers a plain JSON value cannot hold.
    numbers::numeric_values_are_exact();
}

#[test]
fn bad_requests_end_the_worker_with_a_fault_and_the_documented_status() {
    // (what, request that triggers it, expected fault code, expected exit status or None when the ABI sets none)
    let good = json!({"source_id": "g", "generation": 1});
    let other_generation = json!({"source_id": "g", "generation": 2});
    let long_label = json!({"source_id": "x".repeat(1025), "generation": 1});
    let kinds: [&str; 6] = [
        "version",
        "zero-budget",
        "format",
        "label",
        "binding",
        "range",
    ];
    for kind in kinds {
        let mut worker = Worker::start();
        let (fault, status) = match kind {
            "version" => (worker.hello(2, WORK_MS), Some(2)),
            "zero-budget" => (worker.hello(1, 0), None),
            _ => {
                let ready = worker.hello(1, WORK_MS);
                assert_eq!(
                    ready["op"], "ready",
                    "{kind}: handshake before the bad request"
                );
                match kind {
                    "format" => (
                        worker.open(1, &good, "unsupported-provider", "claude-chair-run.jsonl"),
                        Some(2),
                    ),
                    "label" => (
                        worker.open(1, &long_label, "claude", "claude-chair-run.jsonl"),
                        None,
                    ),
                    "binding" => {
                        let opened = worker.open(1, &good, "claude", "claude-chair-run.jsonl");
                        assert_eq!(opened["op"], "opened");
                        let result =
                            worker.snapshot(2, &other_generation, 0, &Value::Null, "snapshot_eof");
                        (result.err().expect("a foreign binding is refused"), Some(2))
                    }
                    _ => {
                        let opened = worker.open(1, &good, "claude", "claude-chair-run.jsonl");
                        assert_eq!(opened["op"], "opened");
                        // Byte 1 is inside the first line, so it is not a line start.
                        let result = worker.snapshot(2, &good, 1, &Value::Null, "snapshot_eof");
                        (
                            result.err().expect("a start inside a line is refused"),
                            Some(2),
                        )
                    }
                }
            }
        };
        let code = match kind {
            "version" => "unsupported_version",
            "zero-budget" | "label" => "bad_request",
            "format" => "unsupported_format",
            "binding" => "binding_mismatch",
            _ => "invalid_range",
        };
        assert_eq!(fault["op"], "fault", "{kind}: reply is a fault");
        assert_eq!(fault["code"], code, "{kind}: fault code");
        // A fault ends the process and no further frame follows it.
        assert!(worker.frame().is_none(), "{kind}: nothing follows a fault");
        let exit = worker.exit_code();
        if let Some(status) = status {
            assert_eq!(exit, status, "{kind}: exit status");
        } else {
            assert_ne!(exit, 0, "{kind}: a fault exits nonzero");
        }
    }
}
