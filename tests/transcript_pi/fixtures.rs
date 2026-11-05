//! Loads the OLD-captured expectations. Nothing here computes an expected value: it joins literal
//! captured data (`fixtures/*.json`, `inputs/*`, listed with their capture in `PROVENANCE.json`)
//! into the lines and rows a step must return.

use std::path::PathBuf;

use serde_json::Value;

use super::client::decode_deep;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/transcript_pi")
}

pub(super) fn input_path(name: &str) -> PathBuf {
    dir().join("inputs").join(name)
}

pub(super) fn read_input(name: &str) -> Vec<u8> {
    let path = input_path(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = dir().join("fixtures").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

pub(super) fn load(name: &str) -> Value {
    decode_deep(&read_fixture(name))
}

/// The OLD native stdout of one captured input, as text (one event per line).
pub(super) fn native_stdout(name: &str) -> String {
    String::from_utf8(read_fixture(name)).expect("OLD stdout is UTF-8")
}

/// The OLD line objects of the main fixture, keyed `start-end`, merged from the two capture halves.
pub(super) fn main_line_map() -> Value {
    let mut merged = serde_json::Map::new();
    for name in ["lines-main-a.json", "lines-main-b.json"] {
        merged.extend(load(name).as_object().expect("line map").clone());
    }
    Value::Object(merged)
}

/// The captured line objects that lie inside `[start, end)`, in file order.
pub(super) fn lines_in(map: &Value, start: u64, end: u64) -> Value {
    let mut picked: Vec<&Value> = map
        .as_object()
        .expect("line map")
        .values()
        .filter(|line| {
            line["start_byte"].as_u64() >= Some(start) && line["end_byte"].as_u64() <= Some(end)
        })
        .collect();
    picked.sort_by_key(|line| line["start_byte"].as_u64());
    Value::Array(picked.into_iter().cloned().collect())
}
