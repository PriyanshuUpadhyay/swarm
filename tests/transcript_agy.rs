//! The private `swarm _transcript-worker --protocol 1` command for AGY logs (unit 05, one main-path
//! test): Open of a log whose path is `.../brain/agy-u04-main/steps.jsonl` under an opaque binding
//! label that is not that path, Snapshot at two origins, two readers over one growing file (append,
//! Advance, a replay that must not disturb the live reader), and the native events of the guard and
//! number files with their exact tool-input tokens.
//!
//! Every expected value is sealed ORIGINAL c8ca837e data (`tests/transcript_agy/PROVENANCE.json` lists
//! each file and its capture). Native events come from the ORIGINAL Zig translator. Row sets are the
//! `rows` arrays of the OLD `GET /api/rows` bodies for the same bytes: the whole file, the 1104-byte
//! prefix that waits on the results of a two-call planner step, the file after those results, the
//! 1104..3476 window, and the 1104..1358 window that holds results only (its OLD HTTP page is empty,
//! its OLD full projection is the two result rows of `old-full-results-only.json`). The guard
//! and number files have no OLD row set (OLD HTTP answers 500 for them and re-spells numbers), so only
//! their native events are asserted: tool-input tokens as raw text, the i64 edges, and the depth edge
//! of 254 arrays (255 gives `unknown`). Requests are literal JSON, so this compiles on a worker that
//! does not know AGY yet; Open is then the first call that fails.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};

#[path = "transcript_agy/client.rs"]
mod client;
#[path = "transcript_agy/fixtures.rs"]
mod fixtures;

use client::Worker;

type Fields = BTreeMap<String, Box<RawValue>>;

/// An opaque binding label. It is not a path and shares no text with the session id.
const LABEL: &str = "opaque-binding-7c1e";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    source_id: String,
    read_end_byte: u64,
    complete_end_byte: u64,
    lines: Vec<SnapshotLine>,
    #[allow(dead_code)]
    rows: Box<RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotLine {
    start_byte: u64,
    end_byte: u64,
    terminated: bool,
    events: Vec<Box<RawValue>>,
}

fn binding() -> Value {
    json!({"source_id": LABEL, "generation": 1})
}

/// Parses text without the recursion limit (the 254-array tool input) and without any number type.
fn parse<'a, T: Deserialize<'a>>(text: &'a str) -> T {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let value = T::deserialize(&mut de).expect("valid JSON of the expected shape");
    de.end().expect("one JSON value");
    value
}

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
            keys.into_iter().all(|k| {
                path.truncate(at);
                path.push_str(&format!(".{k}"));
                match (a.get(k), e.get(k)) {
                    (Some(a), Some(e)) => first_difference(a, e, path),
                    _ => {
                        path.push_str(" [missing on one side]");
                        false
                    }
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

/// Starts a worker and opens `brain/<session>/steps.jsonl` (written under its scratch dir) as an AGY log.
fn open_agy(session: &str, bytes: &[u8], case: &str) -> (Worker, std::path::PathBuf) {
    let mut worker = Worker::start();
    let scratch = worker.scratch.to_str().expect("utf-8 scratch");
    assert!(
        !scratch.split('/').any(|part| part == "brain"),
        "{case}: the scratch path must not hold a brain component"
    );
    let dir = worker.scratch.join("work/brain").join(session);
    std::fs::create_dir_all(&dir).expect("brain dir");
    let path = dir.join("steps.jsonl");
    std::fs::write(&path, bytes).expect("initial bytes");
    let ready = worker.hello();
    assert_eq!(ready["op"], "ready", "{case}: handshake: {ready}");
    let opened = worker.open(&binding(), "agy", path.to_str().expect("utf-8 path"));
    assert_eq!(
        opened,
        json!({"op": "opened", "request_id": worker.last_request_id(), "binding": binding()}),
        "{case}: the worker must accept an AGY log on Open"
    );
    (worker, path)
}

fn check_snapshot(label: &str, got: &Value, end: u64, span: (u64, u64), rows: &str) {
    let mut keys: Vec<&str> = got
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
    assert_eq!(got["source_id"], LABEL, "{label}: source id is the label");
    assert_eq!(got["read_end_byte"], end, "{label}: read end");
    assert_eq!(got["complete_end_byte"], end, "{label}: complete end");
    assert_same(
        label,
        "lines",
        &got["lines"],
        &fixtures::lines(span.0, span.1),
    );
    assert_same(label, "rows", &got["rows"], &fixtures::rows(rows));
}

fn append(path: &Path, bytes: &[u8]) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open for append")
        .write_all(bytes)
        .expect("append");
}

fn step(worker: &mut Worker, name: &str, op: &str, fields: Value, expected: Vec<Value>) {
    let updates = worker
        .reader_op(op, &binding(), fields)
        .unwrap_or_else(|fault| panic!("{name}: the worker answered {op} with a fault: {fault}"));
    assert_same(name, "updates", &updates, &Value::Array(expected));
}

fn reader(id: u64) -> Value {
    json!({"reader_id": id, "generation": 0})
}

/// One native event against the OLD native line: same keys, shallow values equal, and the tool input
/// compared token by token as raw text, so `-0`, `100`, `1e400` and the i64 edges keep their spelling.
fn same_event(label: &str, got: &str, want: &str) {
    let (got, want) = (parse::<Fields>(got), parse::<Fields>(want));
    let (got_keys, want_keys): (Vec<_>, Vec<_>) = (got.keys().collect(), want.keys().collect());
    assert_eq!(got_keys, want_keys, "{label}: event keys");
    for (key, want_raw) in &want {
        let got_raw = &got[key];
        if key == "input" {
            let tokens = |raw: &str| -> Vec<(String, String)> {
                parse::<Fields>(raw)
                    .into_iter()
                    .map(|(name, token)| (name, token.get().to_string()))
                    .collect()
            };
            assert_eq!(
                tokens(got_raw.get()),
                tokens(want_raw.get()),
                "{label}: tool input tokens differ from OLD"
            );
        } else {
            assert_eq!(
                parse::<Value>(got_raw.get()),
                parse::<Value>(want_raw.get()),
                "{label}: event field {key}"
            );
        }
    }
}

/// Snapshots a whole guard or number file and compares its native events, in order, with the OLD
/// native stdout. Rows are not asserted: OLD HTTP has no usable row set for these files.
fn native_events_only(session: &str, input_name: &str, native_name: &str) {
    let bytes = fixtures::input(input_name);
    let (mut worker, _path) = open_agy(session, &bytes, session);
    let text = worker
        .snapshot_raw(&binding(), 0)
        .unwrap_or_else(|fault| panic!("{session}: the worker answered with a fault: {fault}"));
    let mut deep = serde_json::Deserializer::from_str(&text);
    deep.disable_recursion_limit();
    let snapshot = Snapshot::deserialize(&mut deep).expect("snapshot shape");
    assert_eq!(snapshot.source_id, LABEL, "{session}: source id");
    let size = bytes.len() as u64;
    assert_eq!(
        (snapshot.read_end_byte, snapshot.complete_end_byte),
        (size, size),
        "{session}: ends"
    );
    let mut start = 0u64;
    let mut starts = Vec::new();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            starts.push((start, i as u64 + 1));
            start = i as u64 + 1;
        }
    }
    let got_lines: Vec<(u64, u64, bool)> = snapshot
        .lines
        .iter()
        .map(|l| (l.start_byte, l.end_byte, l.terminated))
        .collect();
    let want_lines: Vec<(u64, u64, bool)> = starts.iter().map(|&(s, e)| (s, e, true)).collect();
    assert_eq!(got_lines, want_lines, "{session}: line positions");
    let got: Vec<&str> = snapshot
        .lines
        .iter()
        .flat_map(|l| l.events.iter().map(|e| e.get()))
        .collect();
    let native = fixtures::fixture_text(native_name);
    let want: Vec<&str> = native.lines().collect();
    assert_eq!(got.len(), want.len(), "{session}: event count");
    for (i, (got, want)) in got.iter().zip(&want).enumerate() {
        same_event(&format!("{session}/event {i}"), got, want);
    }
    worker.close_source(&binding());
}

#[test]
fn agy_logs_through_the_worker_match_the_old_translator_and_rows() {
    let main = fixtures::input("agy-u04-main.jsonl");
    assert_eq!(main.len() as u64, fixtures::BOUNDS[21], "the sealed input");

    // Snapshot at two origins over the whole 3476-byte file: the whole read (22 rows) and the
    // continuation window from 1104 (16 rows, no older rows).
    let (mut worker, _) = open_agy("agy-u04-main", &main, "snapshot");
    let whole = worker.snapshot(&binding(), 0).expect("snapshot at 0");
    check_snapshot(
        "snapshot 0",
        &whole.value,
        3476,
        (0, 3476),
        "http-whole.body",
    );
    let window = worker.snapshot(&binding(), 1104).expect("snapshot at 1104");
    check_snapshot(
        "snapshot 1104",
        &window.value,
        3476,
        (1104, 3476),
        "http-window.body",
    );
    worker.close_source(&binding());

    // Two readers over one held file that grows. The file starts after the two-call planner step and
    // before its results, so the first Row set waits; the results turn it into a failed fold.
    let (mut worker, path) = open_agy("agy-u04-main", &main[..1104], "readers");
    let (a, b) = ((1, 0), (2, 0));
    let bind_a = json!({"reader": reader(1), "from_byte": 0});
    step(
        &mut worker,
        "bind-a-waiting-prefix",
        "bind_reader",
        bind_a,
        vec![fixtures::update(
            a,
            (0, 0, 1104),
            1104,
            (0, 1104),
            "http-prefix.body",
        )],
    );
    append(&path, &main[1104..1358]);
    step(
        &mut worker,
        "advance-results-arrive",
        "advance",
        json!({}),
        vec![fixtures::update(
            a,
            (0, 0, 1358),
            1358,
            (1104, 1358),
            "http-after-results.body",
        )],
    );
    // The second reader starts at the results only: its window holds no call. Its full projection is
    // the two OLD result rows (`old-full-results-only.json`, ORIGINAL view_rows_with_context). The OLD
    // HTTP page for this window is empty (`http-result-only.body`): a cold `before=` request lists
    // only changed rows, so that empty page is a page upsert and not this projection.
    let bind_b = json!({"reader": reader(2), "from_byte": 1104});
    step(
        &mut worker,
        "bind-b-results-only",
        "bind_reader",
        bind_b,
        vec![fixtures::update(
            b,
            (1104, 1104, 1358),
            1358,
            (1104, 1358),
            "old-full-results-only.json",
        )],
    );
    // A replay of the older prefix returns its native lines and ends for the range 0..1104, but the
    // reader keeps its retained Row history: the results it has seen stay, so the fold stays failed
    // (the OLD rows of the file after the results), and the live cursor stays at 1358.
    let replay = json!({"reader": reader(1), "start_byte": 0, "end_byte": 1104});
    step(
        &mut worker,
        "replay-a-prefix",
        "replay_reader",
        replay,
        vec![fixtures::update(
            a,
            (0, 0, 1104),
            1358,
            (0, 1104),
            "http-after-results.body",
        )],
    );
    // The rest arrives: reader A ends on the OLD whole-file rows, reader B on the OLD window rows.
    append(&path, &main[1358..]);
    step(
        &mut worker,
        "advance-rest-arrives",
        "advance",
        json!({}),
        vec![
            fixtures::update(a, (0, 0, 3476), 3476, (1358, 3476), "http-whole.body"),
            fixtures::update(
                b,
                (1104, 1104, 3476),
                3476,
                (1358, 3476),
                "http-window.body",
            ),
        ],
    );
    worker.close_source(&binding());

    // Native events of the guard file (i64 edges, partial-then-unknown, depth 254 accepted and 255
    // refused) and of the number file (tool-input tokens), compared as raw text where numbers hide.
    native_events_only(
        "agy-u04-guards",
        "agy-u04-guards.jsonl",
        "native-guards.stdout",
    );
    native_events_only(
        "agy-u04-numbers",
        "agy-u04-numbers.jsonl",
        "native-numbers.stdout",
    );
}
