//! Loads the sealed ORIGINAL AGY data (`PROVENANCE.json` lists each file and its capture). Nothing here
//! computes an expected value: it joins literal captured data by name into the update a step must return.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::client::decode_deep;

/// Input line starts plus the file size (from the sealed capture): lines are `[b[i], b[i + 1])`.
pub(super) const BOUNDS: [u64; 22] = [
    0, 265, 497, 653, 807, 1104, 1232, 1358, 1516, 1650, 1778, 1934, 2070, 2224, 2448, 2573, 2698,
    2826, 2941, 3054, 3180, 3476,
];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/transcript_agy")
}

pub(super) fn input(name: &str) -> Vec<u8> {
    let path = dir().join("inputs").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

pub(super) fn fixture_text(name: &str) -> String {
    let path = dir().join("fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The line objects `[from, to)` of the main file, in order (native events of each input line).
pub(super) fn lines(from: u64, to: u64) -> Value {
    let map = decode_deep(fixture_text("lines-main.json").as_bytes());
    Value::Array(
        BOUNDS
            .windows(2)
            .filter(|w| w[0] >= from && w[1] <= to)
            .map(|w| map[format!("{}-{}", w[0], w[1])].clone())
            .collect(),
    )
}

/// The `rows` array of one captured OLD `GET /api/rows` body.
pub(super) fn rows(name: &str) -> Value {
    decode_deep(fixture_text(name).as_bytes())["rows"].clone()
}

/// The update a reader step must return: all eight keys, with the named line range and row set joined.
pub(super) fn update(
    reader: (u64, u64),
    window: (u64, u64, u64),
    next: u64,
    span: (u64, u64),
    rows_name: &str,
) -> Value {
    json!({
        "reader": {"reader_id": reader.0, "generation": reader.1},
        "start_byte": window.0,
        "origin_byte": window.1,
        "read_end_byte": window.2,
        "complete_end_byte": window.2,
        "next_complete_byte": next,
        "lines": lines(span.0, span.1),
        "rows": rows(rows_name),
    })
}
