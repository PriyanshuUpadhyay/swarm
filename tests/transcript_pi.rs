//! The private `swarm _transcript-worker --protocol 1` command for Pi logs (fifth migration unit):
//! one Snapshot over the whole file and over an output-only window, two readers over one held file
//! that grow by appends, a replay that must not touch live context, and the arithmetic-overflow guard.
//!
//! Every expected value comes from the ORIGINAL c8ca837e Zig translator (ReleaseSafe) and Rust row
//! layer, captured before this unit existed (`tests/transcript_pi/PROVENANCE.json` lists each file and
//! its capture). Requests are literal JSON, so this test compiles with no new production API.
//! Row sets are the `rows` arrays of the OLD `GET /api/rows` bodies for the same byte range.
//!
//! The binding `source_id` is an opaque label that differs from the path-derived Pi session id
//! `pi-u04-main`, which the OLD translator writes into every event meta from the file name alone.
//! Overflow: the OLD translator sums three valid token counts as `i64` and aborts (SIGABRT) after an
//! earlier line was written, and OLD HTTP drops that line. The private worker has no process-signal
//! parity; its terminal contract is a Core fault with status 1 and no frame of the failed operation.

use std::fs::OpenOptions;
use std::io::Write;

use serde_json::{Value, json};

#[path = "transcript_pi/client.rs"]
mod client;
#[path = "transcript_pi/fixtures.rs"]
mod fixtures;
#[path = "transcript_pi/numeric_tokens.rs"]
mod numeric_tokens;

use client::Worker;

/// The file name of the main fixture; the translator derives the session id `pi-u04-main` from it.
const MAIN: &str = "2026-11-05T00-00-00-000Z_pi-u04-main.jsonl";

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

/// Starts a worker, shakes hands and opens `path` as a Pi log. Hello must succeed on the current
/// binary; Open is the first call that a worker without Pi admission refuses.
fn open_pi(worker: &mut Worker, binding: &Value, path: &str, case: &str) {
    let ready = worker.hello();
    assert_eq!(ready["op"], "ready", "{case}: handshake: {ready}");
    let opened = worker.open(binding, "pi", path);
    assert_eq!(
        opened,
        json!({"op": "opened", "request_id": worker.last_request_id(), "binding": binding}),
        "{case}: the worker must accept a Pi log on Open"
    );
}

/// A private copy of the first `end` bytes of the main fixture under its captured file name.
fn growing_copy(worker: &Worker, source: &[u8], end: usize) -> std::path::PathBuf {
    let work = worker.scratch.join("work");
    std::fs::create_dir_all(&work).expect("work dir");
    let path = work.join(MAIN);
    std::fs::write(&path, &source[..end]).expect("initial bytes");
    path
}

/// The snapshot payload keys, binding label and byte ends, then the OLD lines and rows.
fn check_snapshot(
    worker: &mut Worker,
    binding: &Value,
    label: &str,
    start: u64,
    end: u64,
    rows: &Value,
) {
    let committed = worker
        .snapshot(binding, start)
        .unwrap_or_else(|fault| panic!("{label}: the worker answered with a fault: {fault}"));
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
    assert_eq!(payload["read_end_byte"], end, "{label}: read end");
    assert_eq!(payload["complete_end_byte"], end, "{label}: complete end");
    let map = fixtures::main_line_map();
    assert_same(
        label,
        "lines",
        &payload["lines"],
        &fixtures::lines_in(&map, start, end),
    );
    assert_same(label, "rows", &payload["rows"], rows);
}

/// One reader update the OLD data describes: five byte positions, the OLD lines and OLD rows.
fn update(reader: [u64; 2], bytes: [u64; 5], lines: Value, rows: &Value) -> Value {
    let [start, origin, read_end, complete_end, next_complete] = bytes;
    json!({
        "reader": {"reader_id": reader[0], "generation": reader[1]},
        "start_byte": start,
        "origin_byte": origin,
        "read_end_byte": read_end,
        "complete_end_byte": complete_end,
        "next_complete_byte": next_complete,
        "lines": lines,
        "rows": rows,
    })
}

fn append(path: &std::path::Path, source: &[u8], from: usize, to: usize) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open for append")
        .write_all(&source[from..to])
        .expect("append");
}

fn reader_op(worker: &mut Worker, binding: &Value, name: &str, op: &str, fields: Value) -> Value {
    worker
        .reader_op(op, binding, fields)
        .unwrap_or_else(|fault| panic!("{name}: the worker answered {op} with a fault: {fault}"))
}

#[test]
fn pi_logs_through_the_worker_match_the_old_translator_and_rows() {
    let binding = json!({"source_id": "opaque-binding-u04", "generation": 1});
    let source = fixtures::read_input(MAIN);
    let map = fixtures::main_line_map();
    let rows_1265 = fixtures::load("rows-main-at-1265.json");
    let rows_1687 = fixtures::load("rows-main-at-1687.json");
    let rows_4179 = fixtures::load("rows-main-at-4179.json");
    // Canonical full Rows of the window 1265..1687, from the ORIGINAL public Row owner. The sealed
    // HTTP page body `rows-output-only-window.json` (`[]`) stays as the OLD fact: that route lists
    // only rows changed against its history, so empty means "no listed change" there.
    let rows_window_full = fixtures::load("rows-output-only-full.json");
    assert_eq!(source.len(), 4179, "the captured main fixture");

    // Snapshot of the whole tracked file (27 lines, 43 events, 37 rows).
    let mut worker = Worker::start();
    let tracked = fixtures::input_path(MAIN);
    let tracked = tracked.to_str().expect("utf-8 path");
    open_pi(&mut worker, &binding, tracked, "whole");
    check_snapshot(&mut worker, &binding, "whole", 0, 4179, &rows_4179);
    worker.close_source(&binding);

    // Snapshot of an output-only window: the two tool-result lines, without their call. The private
    // Snapshot returns a full Row projection, so it is compared with the OLD full Rows (3 rows).
    let mut worker = Worker::start();
    let path = growing_copy(&worker, &source, 1687);
    open_pi(
        &mut worker,
        &binding,
        path.to_str().expect("utf-8 path"),
        "window",
    );
    check_snapshot(
        &mut worker,
        &binding,
        "window",
        1265,
        1687,
        &rows_window_full,
    );
    worker.close_source(&binding);

    // Two readers over one held file that grows: the prefix, the results, then the rest. The second
    // reader binds at the results state, so each reader continues with its own question state.
    let mut worker = Worker::start();
    let path = growing_copy(&worker, &source, 1265);
    open_pi(
        &mut worker,
        &binding,
        path.to_str().expect("utf-8 path"),
        "readers",
    );
    let (a, b) = ([1, 0], [2, 0]);
    let step = "bind-a-prefix";
    let updates = reader_op(
        &mut worker,
        &binding,
        step,
        "bind_reader",
        json!({"reader": {"reader_id": 1, "generation": 0}, "from_byte": 0}),
    );
    let expected = json!([update(
        a,
        [0, 0, 1265, 1265, 1265],
        fixtures::lines_in(&map, 0, 1265),
        &rows_1265
    )]);
    assert_same(step, "updates", &updates, &expected);

    append(&path, &source, 1265, 1687);
    let step = "append-results-advance";
    let updates = reader_op(&mut worker, &binding, step, "advance", json!({}));
    let expected = json!([update(
        a,
        [0, 0, 1687, 1687, 1687],
        fixtures::lines_in(&map, 1265, 1687),
        &rows_1687
    )]);
    assert_same(step, "updates", &updates, &expected);

    let step = "bind-b-results";
    let updates = reader_op(
        &mut worker,
        &binding,
        step,
        "bind_reader",
        json!({"reader": {"reader_id": 2, "generation": 0}, "from_byte": 0}),
    );
    let expected = json!([update(
        b,
        [0, 0, 1687, 1687, 1687],
        fixtures::lines_in(&map, 0, 1687),
        &rows_1687
    )]);
    assert_same(step, "updates", &updates, &expected);

    append(&path, &source, 1687, 4179);
    let step = "append-rest-advance";
    let updates = reader_op(&mut worker, &binding, step, "advance", json!({}));
    let rest = fixtures::lines_in(&map, 1687, 4179);
    let expected = json!([
        update(a, [0, 0, 4179, 4179, 4179], rest.clone(), &rows_4179),
        update(b, [0, 0, 4179, 4179, 4179], rest, &rows_4179),
    ]);
    assert_same(step, "updates", &updates, &expected);

    // Replay of the older result lines for reader b, which already holds all 4179 bytes. Replay
    // merges the window into b's retained row history (`ReaderRows::merge_page`) and answers with
    // the full retained projection, so b keeps start 0, origin 0, the live next byte 4179 and the
    // 37 OLD rows of the whole file. Only the lines and the read and complete ends describe the
    // window. The stand-alone empty window rows apply to the Snapshot above, not to this replay.
    let step = "replay-window-keeps-retained-rows-for-b";
    let updates = reader_op(
        &mut worker,
        &binding,
        step,
        "replay_reader",
        json!({"reader": {"reader_id": 2, "generation": 0}, "start_byte": 1265, "end_byte": 1687}),
    );
    let expected = json!([update(
        b,
        [0, 0, 1687, 1687, 4179],
        fixtures::lines_in(&map, 1265, 1687),
        &rows_4179
    )]);
    assert_same(step, "updates", &updates, &expected);
    worker.close_source(&binding);

    // Tool-call number tokens, read at the raw response boundary.
    numeric_tokens::numeric_values_are_exact();
}

/// The OLD translator sums the three valid counts of one usage record as `i64` and aborts after it
/// wrote the first line; OLD HTTP then answers 500 with no rows. The private worker must end with a
/// Core fault and status 1 and publish no frame of the failed operation, while the earlier commit
/// stays the last published data. The fault wording and the process signal are not asserted.
#[test]
fn a_pi_token_sum_overflow_ends_the_worker_with_core_and_publishes_nothing() {
    const NAME: &str = "usage-sum-three-2pow62.jsonl";
    let binding = json!({"source_id": "opaque-overflow-u04", "generation": 1});
    let source = fixtures::read_input(NAME);
    let first_end = source
        .iter()
        .position(|&b| b == b'\n')
        .expect("a first line")
        + 1;
    let old_event: Value = serde_json::from_str(
        fixtures::native_stdout("native-usage-sum-three-2pow62.stdout").trim_end(),
    )
    .expect("the one event OLD wrote before it aborted");

    let mut worker = Worker::start();
    let work = worker.scratch.join("work");
    std::fs::create_dir_all(&work).expect("work dir");
    let path = work.join(NAME);
    std::fs::write(&path, &source[..first_end]).expect("first line");
    open_pi(
        &mut worker,
        &binding,
        path.to_str().expect("utf-8 path"),
        "overflow",
    );

    // An earlier operation commits: the first line, exactly as OLD wrote it.
    let committed = worker.snapshot(&binding, 0).unwrap_or_else(|fault| {
        panic!("earlier snapshot: the worker answered with a fault: {fault}")
    });
    let line = json!({"start_byte": 0, "end_byte": first_end, "terminated": true,
        "events": [old_event]});
    assert_same(
        "earlier-snapshot",
        "lines",
        &committed.value["lines"],
        &json!([line]),
    );

    // The overflow record arrives. The failed snapshot would hold the good first line too, so any
    // partial publication shows as a begin frame before the fault.
    append(&path, &source, first_end, source.len());
    let fault = worker
        .snapshot(&binding, 0)
        .err()
        .expect("the overflow ends the operation with a fault and no begin frame");
    assert_eq!(fault["op"], "fault", "reply is a fault: {fault}");
    assert_eq!(fault["code"], "core", "fault code: {fault}");
    assert!(worker.frame().is_none(), "nothing follows a fault");
    assert_eq!(worker.exit_code(), 1, "a Core fault exits with status 1");
}
