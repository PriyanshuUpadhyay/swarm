//! Number tokens of a Pi tool call, run as probes whose failures are collected, so one real CLI run
//! reaches all of them before the final assertion. The ORIGINAL c8ca837e ReleaseSafe translator keeps
//! an integer token at any length, writes `1E2` as `100`, and keeps the non-finite `1e400` as the
//! lowercase raw token (`PROVENANCE.json`; the jq-rendered `1E+400` is not a native value).
//!
//! The payload is read with vendor `RawValue` fields and the selected input field `n` is compared as
//! raw text, so a semantic decoder cannot hide a rounded float for a long integer.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};

use super::client::Worker;
use super::fixtures;

type Fields = BTreeMap<String, Box<RawValue>>;

/// The snapshot as it is on the wire. The five keys are exact; events stay raw text.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSnapshot {
    source_id: String,
    read_end_byte: u64,
    complete_end_byte: u64,
    lines: Vec<RawLine>,
    #[serde(rename = "rows")]
    _rows: Box<RawValue>,
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
}

const PROBES: [Probe; 3] = [
    Probe {
        name: "number-digits30",
        input: "num-digits30.jsonl",
        old_stdout: "native-num-digits30.stdout",
    },
    Probe {
        name: "number-1e400-raw",
        input: "num-f1e400.jsonl",
        old_stdout: "native-num-f1e400.stdout",
    },
    Probe {
        name: "number-whole-float-1E2",
        input: "num-fE2.jsonl",
        old_stdout: "native-num-fE2.stdout",
    },
];

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
    let input = fixtures::input_path(probe.input);
    let source = fixtures::read_input(probe.input);
    let old_stdout = fixtures::native_stdout(probe.old_stdout);
    let old_events: Vec<&str> = old_stdout.lines().collect();

    let binding = json!({"source_id": case, "generation": 1});
    let mut worker = Worker::start();
    assert_eq!(worker.hello()["op"], "ready", "{case}: handshake");
    let opened = worker.open(&binding, "pi", input.to_str().expect("utf-8 path"));
    assert_eq!(opened["op"], "opened", "{case}: open: {opened}");
    let text = worker
        .snapshot_raw(&binding, 0)
        .unwrap_or_else(|fault| panic!("{case}: the worker answered with a fault: {fault}"));
    let snapshot: RawSnapshot = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{case}: the payload is not the snapshot shape: {error}"));
    assert_eq!(snapshot.source_id, case, "{case}: binding label");
    assert_eq!(
        snapshot.read_end_byte,
        source.len() as u64,
        "{case}: read end"
    );
    assert_eq!(
        snapshot.complete_end_byte,
        source.len() as u64,
        "{case}: complete end"
    );
    assert_eq!(snapshot.lines.len(), old_events.len(), "{case}: line count");
    let mut start = 0u64;
    for (line, old) in snapshot.lines.iter().zip(&old_events) {
        let length = source[start as usize..]
            .iter()
            .position(|&byte| byte == b'\n')
            .expect("every captured line ends with a newline") as u64
            + 1;
        assert_eq!(
            (line.start_byte, line.end_byte, line.terminated),
            (start, start + length, true),
            "{case}: line position"
        );
        start += length;
        assert_eq!(line.events.len(), 1, "{case}: one event per line");
        compare_event(case, &fields(line.events[0].get()), &fields(old));
    }
    worker.close_source(&binding);
}

fn compare_event(case: &str, event: &Fields, old_event: &Fields) {
    let got: Vec<&String> = event.keys().collect();
    let want: Vec<&String> = old_event.keys().collect();
    assert_eq!(got, want, "{case}: the event is not the OLD native event");
    for (key, want_raw) in old_event {
        let got_raw = &event[key];
        if key == "input" {
            let (got_input, want_input) = (fields(got_raw.get()), fields(want_raw.get()));
            let want_token = want_input.get("n").expect("OLD input field").get();
            let got_token = got_input.get("n").map(|raw| raw.get());
            assert_eq!(
                got_token,
                Some(want_token),
                "{case}: input raw token n differs from OLD"
            );
        } else {
            let got_value: Value = serde_json::from_str(got_raw.get()).expect("field");
            let want_value: Value = serde_json::from_str(want_raw.get()).expect("field");
            assert_eq!(got_value, want_value, "{case}: event field {key}");
        }
    }
}
