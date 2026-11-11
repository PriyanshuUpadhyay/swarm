//! Two more value-sets for the Codex main test, run as probes whose failures are collected, so one
//! real CLI run reaches both before the final assertion:
//!
//! 1. Function-call arguments `{"negzero":-0,"whole_float":1E2}`. The ORIGINAL c8ca837e Zig
//!    translator keeps `-0` and writes `100` (`PROVENANCE-numeric-tokens.json`).
//! 2. A `task_complete` record with `duration_ms:-0`. The ORIGINAL translator emits `turn_ended`
//!    with `duration_ms:null` (`PROVENANCE-scalar-negzero.json`).
//!
//! The payload is read with vendor `RawValue` fields, and the selected input fields are compared as
//! raw text, so a semantic decoder cannot hide `0` for `-0` or `100.0` for `100`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};

use super::client::Worker;

type Fields = BTreeMap<String, Box<RawValue>>;

/// The snapshot as it is on the wire. The five keys are exact; events and rows stay raw text.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSnapshot {
    source_id: String,
    read_end_byte: u64,
    complete_end_byte: u64,
    lines: Vec<RawLine>,
    #[allow(dead_code)]
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

struct Probe {
    name: &'static str,
    input: &'static str,
    old_stdout: &'static str,
    /// Input fields compared as raw text (the `input` of a tool call). Empty when the event has none.
    tokens: &'static [&'static str],
}

const PROBES: [Probe; 2] = [
    Probe {
        name: "number-negzero-whole-float",
        input: "inputs/codex-negzero-whole.jsonl",
        old_stdout: "fixtures/native-negzero-whole.stdout",
        tokens: &["negzero", "whole_float"],
    },
    Probe {
        name: "number-duration-negzero",
        input: "inputs/codex-duration-negzero.jsonl",
        old_stdout: "fixtures/native-duration-negzero.stdout",
        tokens: &[],
    },
];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/transcript_codex")
}

fn fields(raw: &str) -> Fields {
    serde_json::from_str(raw).expect("one JSON object")
}

/// Runs every probe and reports every mismatch, so one wrong value cannot hide another.
pub fn numeric_values_are_exact() {
    let mut failures = Vec::new();
    for probe in &PROBES {
        if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(probe)))
        {
            let text = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            failures.push(format!("{}: {}", probe.name, text));
        }
    }
    assert!(
        failures.is_empty(),
        "numeric values differ:\n{}",
        failures.join("\n")
    );
}

fn check(probe: &Probe) {
    let case = probe.name;
    let input = dir().join(probe.input);
    let size = std::fs::metadata(&input).expect("input").len();
    let old_stdout = std::fs::read_to_string(dir().join(probe.old_stdout)).expect("OLD");
    let old_event = fields(old_stdout.strip_suffix('\n').expect("one OLD event line"));

    let binding = json!({"source_id": case, "generation": 1});
    let mut worker = Worker::start();
    assert_eq!(worker.hello()["op"], "ready", "{case}: handshake");
    let opened = worker.open(&binding, "codex", input.to_str().expect("utf-8 path"));
    assert_eq!(opened["op"], "opened", "{case}: open: {opened}");
    let committed = worker
        .snapshot(&binding, 0)
        .unwrap_or_else(|fault| panic!("{case}: the worker answered with a fault: {fault}"));
    let snapshot: RawSnapshot = serde_json::from_str(&committed.text)
        .unwrap_or_else(|error| panic!("{case}: the payload is not the snapshot shape: {error}"));
    assert_eq!(snapshot.source_id, case, "{case}: binding label");
    assert_eq!(snapshot.read_end_byte, size, "{case}: read end");
    assert_eq!(snapshot.complete_end_byte, size, "{case}: complete end");
    assert_eq!(snapshot.lines.len(), 1, "{case}: one line");
    let line = &snapshot.lines[0];
    assert_eq!(
        (line.start_byte, line.end_byte, line.terminated),
        (0, size, true),
        "{case}: line position"
    );
    assert_eq!(line.events.len(), 1, "{case}: one event");

    let event = fields(line.events[0].get());
    let got: Vec<&String> = event.keys().collect();
    let want: Vec<&String> = old_event.keys().collect();
    assert_eq!(got, want, "{case}: the event is not the OLD native event");
    for (key, want_raw) in &old_event {
        let got_raw = &event[key];
        if key == "input" && !probe.tokens.is_empty() {
            let (got_input, want_input) = (fields(got_raw.get()), fields(want_raw.get()));
            let mut differences = Vec::new();
            for name in probe.tokens {
                let want_token = want_input.get(*name).expect("OLD input field").get();
                let got_token = got_input.get(*name).map(|raw| raw.get());
                if got_token != Some(want_token) {
                    differences.push(format!("{name}: got {got_token:?}, OLD {want_token:?}"));
                }
            }
            assert!(
                differences.is_empty(),
                "{case}: input raw tokens differ from OLD: {}",
                differences.join("; ")
            );
        } else {
            let got_value: Value = serde_json::from_str(got_raw.get()).expect("field");
            let want_value: Value = serde_json::from_str(want_raw.get()).expect("field");
            assert_eq!(got_value, want_value, "{case}: event field {key}");
        }
    }
    worker.close_source(&binding);
}
