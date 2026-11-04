//! Loads the OLD-captured expectations of the reader tests. Nothing here computes an expected
//! value: it only joins literal captured data by name (`rows.json`, `native-lines.json`) into the
//! update the worker must send.

use std::path::PathBuf;

use serde_json::{Value, json};

pub(super) fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transcript-readers")
}

fn load(name: &str) -> Value {
    let path = fixture_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

pub(super) struct Scenario {
    pub(super) steps: Vec<Value>,
    pub(super) binding: Value,
    pub(super) decoy_line: String,
    pub(super) source: Vec<u8>,
    lines: Value,
    rows: Value,
}

impl Scenario {
    pub(super) fn load() -> Scenario {
        let steps = load("steps.json");
        // The bytes are the tracked OLD fixture, read as is; offsets below index into them.
        let source = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/transcript-worker/inputs")
                .join(steps["source"].as_str().expect("source name")),
        )
        .expect("the tracked claude fixture");
        Scenario {
            binding: steps["binding"].clone(),
            decoy_line: steps["decoy_line"].as_str().expect("decoy").to_string(),
            steps: steps["steps"].as_array().expect("steps").clone(),
            source,
            lines: load("native-lines.json"),
            rows: load("rows.json"),
        }
    }

    /// The updates a step must return, with named line ranges and row sets replaced by the data.
    pub(super) fn expected_updates(&self, step: &Value) -> Value {
        let updates = step["expect"].as_array().expect("expect");
        Value::Array(
            updates
                .iter()
                .map(|update| {
                    let mut update = update.clone();
                    update["reader"] = json!({"reader_id": update["reader"][0], "generation": update["reader"][1]});
                    let ranges = update["lines"].as_array().expect("line ranges").clone();
                    update["lines"] = Value::Array(
                        ranges
                            .iter()
                            .map(|range| {
                                let key = range.as_str().expect("range key");
                                assert!(self.lines.get(key).is_some(), "unknown line range {key}");
                                self.lines[key].clone()
                            })
                            .collect(),
                    );
                    let name = update["rows"].as_str().expect("rows name").to_string();
                    assert!(self.rows.get(&name).is_some(), "unknown row set {name}");
                    update["rows"] = self.rows[&name].clone();
                    update
                })
                .collect(),
        )
    }
}
