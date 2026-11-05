//! Loads the OLD-captured expectations. Nothing here computes an expected value: it joins literal
//! captured data by name (`lines-*.json`, `rows-*.json`) into the update a step must return.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::client::decode_deep;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/transcript_codex")
}

pub(super) fn input_path(name: &str) -> PathBuf {
    dir().join("inputs").join(name)
}

fn load(name: &str) -> Value {
    let path = dir().join("fixtures").join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    decode_deep(&bytes)
}

pub(super) fn steps() -> Value {
    load("steps.json")
}

/// The additive snapshot group of a tool_search pair whose tool JSON holds 42.0 and 1e-7.
pub(super) fn steps_tool_search() -> Value {
    load("steps-tool-search.json")
}

/// The additive reader phase in which the question call is open but its output has not arrived.
pub(super) fn steps_pending_question() -> Value {
    load("steps-pending-question.json")
}

/// The line objects of one origin, keyed `start-end`, merged from the named files.
pub(super) fn line_map(files: &Value) -> Value {
    let mut merged = serde_json::Map::new();
    for name in files.as_array().expect("line files") {
        let part = load(name.as_str().expect("file name"));
        merged.extend(part.as_object().expect("line map").clone());
    }
    Value::Object(merged)
}

pub(super) fn lines(map: &Value, ranges: &Value) -> Value {
    Value::Array(
        ranges
            .as_array()
            .expect("ranges")
            .iter()
            .map(|range| {
                let key = range.as_str().expect("range key");
                map.get(key)
                    .unwrap_or_else(|| panic!("unknown line range {key}"))
                    .clone()
            })
            .collect(),
    )
}

pub(super) fn rows(name: &Value) -> Value {
    load(name.as_str().expect("row set name"))
}

/// The update a reader step must return, with named line ranges and row sets replaced by the data.
pub(super) fn expected_update(update: &Value, maps: &Value) -> Value {
    json!({
        "reader": {"reader_id": update["reader"][0], "generation": update["reader"][1]},
        "start_byte": update["start_byte"],
        "origin_byte": update["origin_byte"],
        "read_end_byte": update["read_end_byte"],
        "complete_end_byte": update["complete_end_byte"],
        "next_complete_byte": update["next_complete_byte"],
        "lines": lines(&maps[update["lines_from"].as_str().expect("origin")], &update["lines"]),
        "rows": rows(&update["rows"]),
    })
}
