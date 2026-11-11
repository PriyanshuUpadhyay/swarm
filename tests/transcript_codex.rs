//! The private `swarm _transcript-worker --protocol 1` command for Codex logs (third migration
//! unit): Snapshot at three origins, two or three readers over one held file, a replay that must not
//! touch live context, and an append that each reader continues with its own question state.
//!
//! Every expected value comes from the ORIGINAL c8ca837e Zig translator and Rust row layer, captured
//! before this unit existed (`tests/transcript_codex/PROVENANCE.json` lists each file and its capture).
//! Requests are literal JSON, so this test compiles with no new production API. Row sets are the
//! `rows` arrays of the OLD `GET /api/rows` bodies. The original HTTP route refuses an argument
//! nested 126 arrays or deeper (HTTP 500); the private worker has its own depth policy, so the
//! 254-array input below asserts native lines only and no private rows.

use std::fs::OpenOptions;
use std::io::Write;

use serde_json::{Value, json};

#[path = "transcript_codex/client.rs"]
mod client;
#[path = "transcript_codex/fixtures.rs"]
mod fixtures;
#[path = "transcript_codex/numeric_tokens.rs"]
mod numeric_tokens;

use client::Worker;

fn first_difference(actual: &Value, expected: &Value, path: &mut String) -> bool {
    let at = path.len();
    match (actual, expected) {
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                path.push_str(&format!("[len {} != {}]", a.len(), e.len()));
                return false;
            }
            a.iter().zip(e).enumerate().all(|(i, (a, e))| {
                path.truncate(at);
                path.push_str(&format!("[{i}]"));
                first_difference(a, e, path)
            })
        }
        (Value::Object(a), Value::Object(e)) => {
            let mut keys: Vec<&String> = a.keys().chain(e.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().all(|k| match (a.get(k), e.get(k)) {
                (Some(a), Some(e)) => {
                    path.truncate(at);
                    path.push_str(&format!(".{k}"));
                    first_difference(a, e, path)
                }
                _ => {
                    path.truncate(at);
                    path.push_str(&format!(".{k} [missing on one side]"));
                    false
                }
            })
        }
        _ => actual == expected,
    }
}

fn assert_same(case: &str, field: &str, actual: &Value, expected: &Value) {
    let mut path = String::new();
    assert!(
        first_difference(actual, expected, &mut path),
        "{case}: {field} differs from the OLD value at {path}"
    );
}

/// Starts a worker, shakes hands and opens `path` as a Codex log. Hello must succeed on the
/// current binary; Open is the first call that a Claude-only worker refuses.
fn open_codex(binding: &Value, path: &str, case: &str) -> Worker {
    let mut worker = Worker::start();
    let ready = worker.hello();
    assert_eq!(ready["op"], "ready", "{case}: handshake: {ready}");
    let opened = worker.open(binding, "codex", path);
    assert_eq!(
        opened,
        json!({"op": "opened", "request_id": worker.last_request_id(), "binding": binding}),
        "{case}: the worker must accept a Codex log on Open"
    );
    worker
}

/// Runs snapshot groups: each opens its tracked file as a Codex log, takes the snapshots and
/// compares native lines and OLD rows (and exact digits where a group lists them).
fn run_snapshot_groups(binding: &Value, groups: &Value) {
    for group in groups.as_array().expect("groups") {
        let name = group["name"].as_str().expect("group name");
        let path = fixtures::input_path(group["input"].as_str().expect("input"));
        let mut worker = open_codex(binding, path.to_str().expect("utf-8 path"), name);
        let maps: Value = group["lines_files"]
            .as_object()
            .expect("line files")
            .iter()
            .map(|(origin, files)| (origin.clone(), fixtures::line_map(files)))
            .collect::<serde_json::Map<_, _>>()
            .into();
        for snapshot in group["snapshots"].as_array().expect("snapshots") {
            let label = format!("{name}/{}", snapshot["label"].as_str().expect("label"));
            let committed = worker
                .snapshot(binding, snapshot["start"].as_u64().expect("start"))
                .unwrap_or_else(|fault| {
                    panic!("{label}: the worker answered with a fault: {fault}")
                });
            let payload = &committed.value;
            let mut keys: Vec<&str> = payload
                .as_object()
                .expect("snapshot object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort();
            assert_eq!(
                keys,
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
                payload["source_id"], binding["source_id"],
                "{label}: source id"
            );
            assert_eq!(
                payload["read_end_byte"], snapshot["end"],
                "{label}: read end"
            );
            assert_eq!(
                payload["complete_end_byte"], snapshot["end"],
                "{label}: complete end"
            );
            let origin = snapshot["lines_from"].as_str().expect("origin");
            assert_same(
                &label,
                "lines",
                &payload["lines"],
                &fixtures::lines(&maps[origin], &snapshot["lines"]),
            );
            // A null row set means the OLD HTTP route could not answer this input, so no private
            // row value is asserted for it.
            if !snapshot["rows"].is_null() {
                assert_same(
                    &label,
                    "rows",
                    &payload["rows"],
                    &fixtures::rows(&snapshot["rows"]),
                );
            }
            for digits in snapshot["payload_has"].as_array().into_iter().flatten() {
                let digits = digits.as_str().expect("digits");
                assert!(
                    committed.text.contains(digits),
                    "{label}: native digits {digits} stay exact"
                );
            }
            for digits in snapshot["payload_lacks"].as_array().into_iter().flatten() {
                let digits = digits.as_str().expect("digits");
                assert!(
                    !committed.text.contains(digits),
                    "{label}: no rounded form {digits}"
                );
            }
        }
        worker.close_source(binding);
    }
}

#[test]
fn codex_logs_through_the_worker_match_the_old_translator_and_rows() {
    let spec = fixtures::steps();
    let binding = spec["binding"].clone();

    // Snapshot groups: the composite kernel file at three origins, the numbers and depth guards, and
    // the async yielded-text and exit-code-3 trace.
    run_snapshot_groups(&binding, &spec["snapshot_groups"]);

    // Readers, phase 1: three origins over one held file, append, replay, and a quiet advance.
    run_readers(&binding, &spec["readers"], "readers");
    // Readers, phase 2: the question call is open and its output has not arrived yet. A replay whose
    // window excludes the call must not replace the live context, and the output that arrives
    // after it is hidden for the readers that know the call and shown for the one that does not.
    run_readers(
        &binding,
        &fixtures::steps_pending_question()["readers"],
        "pending-question",
    );
    // Tool JSON inside a tool_search output keeps the OLD number spelling (42, not 42.0).
    run_snapshot_groups(&binding, &fixtures::steps_tool_search()["snapshot_groups"]);
    // The exact tokens -0 and 100 in a tool input, and a scalar duration of -0, read at the raw
    // response boundary.
    numeric_tokens::numeric_values_are_exact();
}

/// Runs one reader phase over a growing copy of a tracked Codex file. Every step is a literal
/// request; every expected update is the OLD data joined by name.
fn run_readers(binding: &Value, readers: &Value, label: &str) {
    let maps: Value = [
        (
            "o0",
            json!(["lines-kernel-o0-a.json", "lines-kernel-o0-b.json"]),
        ),
        ("o1752", json!(["lines-kernel-o1752.json"])),
        ("o2549", json!(["lines-kernel-o2549.json"])),
    ]
    .into_iter()
    .map(|(origin, files)| (origin.to_string(), fixtures::line_map(&files)))
    .collect::<serde_json::Map<_, _>>()
    .into();
    let source = std::fs::read(fixtures::input_path(
        readers["input"].as_str().expect("input"),
    ))
    .expect("the tracked codex fixture");
    let initial_end = readers["initial_end"].as_u64().expect("initial end") as usize;
    // The worker's scratch dir holds the growing copy; it is removed when the worker is dropped.
    let mut worker = Worker::start();
    let work = worker.scratch.join("work");
    std::fs::create_dir_all(&work).expect("work dir");
    let path = work.join("chair.jsonl");
    std::fs::write(&path, &source[..initial_end]).expect("initial bytes");
    assert_eq!(worker.hello()["op"], "ready");
    let opened = worker.open(binding, "codex", path.to_str().expect("utf-8 path"));
    assert_eq!(
        opened,
        json!({"op": "opened", "request_id": worker.last_request_id(), "binding": binding}),
        "{label}: the worker must accept a Codex log on Open"
    );
    for step in readers["steps"].as_array().expect("steps") {
        let name = step["name"].as_str().expect("step name");
        if let Some(range) = step["append"].as_array() {
            let (from, to) = (
                range[0].as_u64().unwrap() as usize,
                range[1].as_u64().unwrap() as usize,
            );
            OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open for append")
                .write_all(&source[from..to])
                .expect("append");
        }
        let op = step["op"].as_str().expect("op");
        let mut fields = json!({});
        if !step["reader"].is_null() {
            fields["reader"] =
                json!({"reader_id": step["reader"][0], "generation": step["reader"][1]});
        }
        for key in ["from_byte", "start_byte", "end_byte"] {
            if !step[key].is_null() {
                fields[key] = step[key].clone();
            }
        }
        let updates = worker
            .reader_op(op, binding, fields)
            .unwrap_or_else(|fault| {
                panic!("{label}/{name}: the worker answered {op} with a fault: {fault}")
            });
        let expected: Value = step["expect"]
            .as_array()
            .expect("expect")
            .iter()
            .map(|update| fixtures::expected_update(update, &maps))
            .collect::<Vec<_>>()
            .into();
        assert_same(&format!("{label}/{name}"), "updates", &updates, &expected);
    }
    worker.close_source(binding);
}
