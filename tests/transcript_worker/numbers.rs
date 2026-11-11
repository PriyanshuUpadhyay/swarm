//! Native numbers that a plain `serde_json::Value` cannot hold or spells differently, through the
//! same CLI main path.
//!
//! The OLD Zig translator keeps `1e400` and `18446744073709551616` in a tool input exactly, keeps
//! `-0`, writes `1E2` as `100`, turns a `-0` duration into `null`, and writes a cost of `0` and of
//! `15.275728604913303` with those exact digits. The expected bytes are the OLD native stdout
//! (`PROVENANCE*.json` list the source and hash). Nothing here parses those numbers to a float or an
//! integer: the payload is read with vendor `RawValue` fields, and a named `input` field or a named
//! top-level field is compared as raw text before any semantic decoding.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::value::RawValue;

use super::{Value, WORK_MS, Worker, assert_same, decode_deep, fixture_dir, json};

type Fields = BTreeMap<String, Box<RawValue>>;

/// The snapshot as it is on the wire. The five keys are exact; events and rows stay raw text.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSnapshot {
    source_id: String,
    read_end_byte: u64,
    complete_end_byte: u64,
    lines: Vec<RawLine>,
    rows: Box<RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLine {
    start_byte: u64,
    end_byte: u64,
    terminated: bool,
    events: Vec<Box<RawValue>>,
}

enum Rows {
    /// The OLD `/api/rows` body for the same input (round 9). Its `rows` array is the expected value.
    OldHttp(&'static str),
    /// A valid native event that the row owner's number type cannot hold: the amended ABI shape.
    Unavailable,
    /// No OLD row set was captured for this value-set, so none is asserted.
    NotCaptured,
}

struct Case {
    name: &'static str,
    input: &'static str,
    native_stdout: &'static str,
    rows: Rows,
    /// Empty: the whole `input` text is compared. Otherwise only these input fields are compared,
    /// each as its raw token.
    tokens: &'static [&'static str],
    /// Top-level event fields compared as raw text before the other fields are decoded.
    raw_fields: &'static [&'static str],
}

const CASES: [Case; 6] = [
    Case {
        name: "number-1e400",
        input: "numbers-input-1e400.jsonl",
        native_stdout: "numbers/native-1e400.stdout",
        rows: Rows::Unavailable,
        tokens: &[],
        raw_fields: &[],
    },
    Case {
        name: "number-2-pow-64",
        input: "numbers-input-u64-plus-1.jsonl",
        native_stdout: "numbers/native-u64-plus-1.stdout",
        rows: Rows::OldHttp("numbers/old-http-rows-u64-plus-1.body"),
        tokens: &[],
        raw_fields: &[],
    },
    // Source `{"negzero":-0,"whole_float":1E2}`: the OLD native input keeps `-0` and writes `100`.
    // Data: Pi checker's ORIGINAL capture (`PROVENANCE-numeric-tokens.json`).
    Case {
        name: "number-negzero-whole-float",
        input: "numbers-input-negzero-whole-float.jsonl",
        native_stdout: "numbers/native-negzero-whole-float.stdout",
        rows: Rows::NotCaptured,
        tokens: &["negzero", "whole_float"],
        raw_fields: &[],
    },
    // Source `durationMs:-0` on a turn_duration record: the OLD native event has `duration_ms:null`
    // (the other fields are compared as in every case). Data: Pi checker's ORIGINAL capture
    // (`PROVENANCE-scalar-negzero.json`).
    Case {
        name: "number-duration-negzero",
        input: "numbers-input-duration-negzero.jsonl",
        native_stdout: "numbers/native-duration-negzero.stdout",
        rows: Rows::NotCaptured,
        tokens: &[],
        raw_fields: &[],
    },
    // Claude `cost-state` with `totalCostUSD:0`: the OLD native `cost_usd` is the token `0`.
    // Data: ORIGINAL capture of round 6 (`PROVENANCE-cost-v4.json`).
    Case {
        name: "number-cost-zero",
        input: "numbers-input-cost-zero.jsonl",
        native_stdout: "numbers/native-cost-zero.stdout",
        rows: Rows::NotCaptured,
        tokens: &[],
        raw_fields: &["cost_usd"],
    },
    // The same field with `15.275728604913303`: the OLD native `cost_usd` keeps those digits.
    Case {
        name: "number-cost-decimal",
        input: "numbers-input-cost-decimal.jsonl",
        native_stdout: "numbers/native-cost-decimal.stdout",
        rows: Rows::NotCaptured,
        tokens: &[],
        raw_fields: &["cost_usd"],
    },
];

fn fields(raw: &str) -> Fields {
    serde_json::from_str(raw).expect("one JSON object")
}

fn short(raw: &str) -> String {
    raw.chars().take(160).collect()
}

/// Compares the named input fields as raw text, so `-0` cannot pass as `0` and `100` cannot pass as
/// `1E2` or `100.0`. A name missing on the OLD side fails the test.
fn selected_tokens(case: &Case, got: &str, want: &str) {
    let (got, want) = (fields(got), fields(want));
    let mut differences = Vec::new();
    for name in case.tokens {
        let want_raw = want.get(*name).expect("the OLD input has the field").get();
        let got_raw = got.get(*name).map(|raw| raw.get());
        if got_raw != Some(want_raw) {
            differences.push(format!("{name}: got {got_raw:?}, OLD {want_raw:?}"));
        }
    }
    assert!(
        differences.is_empty(),
        "{}: input raw tokens differ from OLD: {}",
        case.name,
        differences.join("; ")
    );
}

/// Runs every value and reports every mismatch, so one wrong value cannot hide another.
pub fn numeric_values_are_exact() {
    let mut failures = Vec::new();
    for case in &CASES {
        if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(case))) {
            let text = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            failures.push(format!("{}: {}", case.name, text));
        }
    }
    assert!(
        failures.is_empty(),
        "numeric values differ:\n{}",
        failures.join("\n")
    );
}

fn check(case: &Case) {
    {
        let source = std::fs::read(fixture_dir().join("inputs").join(case.input)).expect("input");
        let size = source.len() as u64;
        let old_stdout =
            std::fs::read_to_string(fixture_dir().join(case.native_stdout)).expect("OLD stdout");
        let old_event = old_stdout.strip_suffix('\n').expect("one OLD event line");
        let old_fields = fields(old_event);

        let binding = json!({"source_id": case.name, "generation": 1});
        let mut worker = Worker::start();
        let ready = worker.hello(1, WORK_MS);
        assert_eq!(ready["op"], "ready", "{}: handshake", case.name);
        let opened = worker.open(1, &binding, "claude", case.input);
        assert_eq!(opened["op"], "opened", "{}: open", case.name);
        let committed = worker
            .snapshot_raw(2, &binding, 0, &Value::Null, "snapshot_eof")
            .unwrap_or_else(|fault| {
                panic!("{}: the worker answered with a fault: {fault}", case.name)
            });
        let snapshot: RawSnapshot =
            serde_json::from_slice(&committed.payload).unwrap_or_else(|error| {
                panic!(
                    "{}: the payload is not the snapshot shape: {error}",
                    case.name
                )
            });

        assert_eq!(
            snapshot.source_id, case.name,
            "{}: binding label",
            case.name
        );
        assert_eq!(snapshot.read_end_byte, size, "{}: read end", case.name);
        assert_eq!(
            snapshot.complete_end_byte, size,
            "{}: complete end",
            case.name
        );
        assert_eq!(snapshot.lines.len(), 1, "{}: one line", case.name);
        let line = &snapshot.lines[0];
        assert_eq!(
            (line.start_byte, line.end_byte, line.terminated),
            (0, size, true),
            "{}: line position",
            case.name
        );
        assert_eq!(line.events.len(), 1, "{}: one event", case.name);

        // The native event: same keys, same metadata and shape, and the exact input text.
        let event = fields(line.events[0].get());
        let got: Vec<&String> = event.keys().collect();
        let want: Vec<&String> = old_fields.keys().collect();
        assert_eq!(
            got,
            want,
            "{}: the event is not the OLD native event (its type is {}, input starts {:?})",
            case.name,
            event.get("type").map_or("<none>", |t| t.get()),
            event
                .get("input")
                .map(|i| short(i.get()))
                .unwrap_or_default()
        );
        for (key, want_raw) in &old_fields {
            let got_raw = &event[key];
            if key == "input" && !case.tokens.is_empty() {
                selected_tokens(case, got_raw.get(), want_raw.get());
            } else if key == "input" {
                assert_eq!(
                    got_raw.get(),
                    want_raw.get(),
                    "{}: the native tool input differs from OLD",
                    case.name
                );
            } else if case.raw_fields.contains(&key.as_str()) {
                assert_eq!(
                    got_raw.get(),
                    want_raw.get(),
                    "{}: event field {key} differs from the OLD raw token",
                    case.name
                );
            } else {
                let got_value: Value = serde_json::from_str(got_raw.get()).expect("field");
                let want_value: Value = serde_json::from_str(want_raw.get()).expect("field");
                assert_same(case.name, key, &got_value, &want_value);
            }
        }

        let rows: Value = serde_json::from_str(snapshot.rows.get()).expect("rows");
        match case.rows {
            Rows::Unavailable => {
                assert_eq!(
                    rows,
                    json!({"error": "unrepresentable_number"}),
                    "{}: rows are explicitly unavailable",
                    case.name
                );
            }
            Rows::NotCaptured => {}
            Rows::OldHttp(path) => {
                let body =
                    decode_deep(&std::fs::read(fixture_dir().join(path)).expect("OLD rows body"));
                assert_same(case.name, "rows", &rows, &body["rows"]);
            }
        }
        worker.close(3, &binding);
    }
}
