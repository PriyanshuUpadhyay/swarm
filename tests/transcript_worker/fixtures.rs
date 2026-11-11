//! Fixture loading and the assertions on fixture data. The data are OLD captures; see PROVENANCE.json.

use std::path::PathBuf;

use serde::Deserialize as _;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transcript-worker")
}

/// Decodes JSON that may nest deeper than serde_json's default limit. Only the worker's own
/// bounded output and the fixture data go through this.
pub(super) fn decode_deep(bytes: &[u8]) -> Value {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    de.disable_recursion_limit();
    let value = Value::deserialize(&mut de).expect("valid JSON");
    de.end().expect("one JSON value");
    value
}

/// True when the two JSON values are equal. When they differ, `path` names the first difference.
pub(super) fn first_difference(actual: &Value, expected: &Value, path: &mut String) -> bool {
    let at = path.len();
    match (actual, expected) {
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                path.push_str(&format!("[len {} != {}]", a.len(), e.len()));
                false
            } else {
                a.iter().zip(e).enumerate().all(|(i, (a, e))| {
                    path.truncate(at);
                    path.push_str(&format!("[{i}]"));
                    first_difference(a, e, path)
                })
            }
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

pub(super) fn assert_same(case: &str, field: &str, actual: &Value, expected: &Value) {
    let mut path = String::new();
    assert!(
        first_difference(actual, expected, &mut path),
        "{case}: {field} differs from the OLD value at {path}"
    );
}

/// The text of the original giant-output guard: `line 0\n` up to `line {count - 1}\n`.
pub(super) fn numbered_lines(count: u64) -> String {
    (0..count).map(|index| format!("line {index}\n")).collect()
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Writes the large OLD-captured source from its recipe and returns its path. The bytes must
/// match the hash captured from the OLD run, so the recipe cannot drift from the captured value.
pub(super) fn materialize_large_source(recipe: &Value, dir: &std::path::Path) -> String {
    let full = numbered_lines(recipe["lines"].as_u64().expect("lines"));
    let call = recipe["call_line"].as_str().expect("call line");
    let result = format!(
        r#"{{"type":"user","uuid":"result","message":{{"content":[{{"type":"tool_result","tool_use_id":"giant","content":"{}"}}]}}}}"#,
        full.replace('\n', "\\n")
    );
    let source = format!("{call}\n{result}\n");
    assert_eq!(
        sha256_hex(source.as_bytes()),
        recipe["source_sha256"].as_str().expect("source hash"),
        "the recipe makes the source bytes that OLD translated"
    );
    assert_eq!(
        sha256_hex(full.as_bytes()),
        recipe["full_text_sha256"].as_str().expect("text hash"),
        "the expanded text is the text that OLD returned"
    );
    let path = dir.join(recipe["file"].as_str().expect("file name"));
    std::fs::write(&path, source).expect("write the large source");
    path.to_str().expect("utf-8 path").to_string()
}

pub(super) fn expand_recipes(group: &mut Value) {
    let recipes = group["expand"].as_array().cloned().unwrap_or_default();
    for recipe in recipes {
        let text = match recipe["numbered_lines"].as_u64() {
            Some(count) => numbered_lines(count),
            None => recipe["repeat"]
                .as_str()
                .expect("repeat unit")
                .repeat(recipe["count"].as_u64().expect("count") as usize),
        };
        let line = recipe["line"].as_u64().expect("line") as usize;
        let event = recipe["event"].as_u64().expect("event") as usize;
        let field = recipe["field"].as_str().expect("field");
        for snapshot in group["snapshots"].as_array_mut().expect("snapshots") {
            snapshot["lines"][line]["events"][event][field] = Value::String(text.clone());
        }
    }
}

/// The expected data: a small index plus the snapshot files it names. A snapshot entry `{"ref": path}`
/// is replaced by the parsed content of that file, so the value is the one the single file held.
pub(super) fn load_expected() -> Value {
    let mut expected =
        decode_deep(&std::fs::read(fixture_dir().join("expected.json")).expect("expected.json"));
    for group in expected["groups"].as_array_mut().expect("groups") {
        for snapshot in group["snapshots"].as_array_mut().expect("snapshots") {
            if let Some(path) = snapshot
                .get("ref")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                *snapshot =
                    decode_deep(&std::fs::read(fixture_dir().join(path)).expect("snapshot file"));
            }
        }
    }
    expected
}

/// Builds a source from a recipe of text parts and repeated units, then checks its byte count and
/// sha256 against the values captured from the original file before the worker opens it.
fn materialize_parts(recipe: &Value, dir: &std::path::Path) -> String {
    let mut source = String::new();
    for part in recipe["parts"].as_array().expect("parts") {
        match part["text"].as_str() {
            Some(text) => source.push_str(text),
            None => source.push_str(
                &part["repeat"]
                    .as_str()
                    .expect("repeat unit")
                    .repeat(part["count"].as_u64().expect("count") as usize),
            ),
        }
    }
    assert_eq!(
        source.len() as u64,
        recipe["bytes"].as_u64().expect("bytes"),
        "the recipe makes the original byte count"
    );
    assert_eq!(
        sha256_hex(source.as_bytes()),
        recipe["sha256"].as_str().expect("hash"),
        "the recipe makes the original bytes"
    );
    let path = dir.join(recipe["file"].as_str().expect("file name"));
    std::fs::write(&path, source).expect("write the source");
    path.to_str().expect("utf-8 path").to_string()
}

/// A group with a `materialize` recipe gets its source built in the worker's scratch dir.
pub(super) fn materialize_source(recipe: &Value, dir: &std::path::Path) -> String {
    if recipe.get("parts").is_some() {
        materialize_parts(recipe, dir)
    } else {
        materialize_large_source(recipe, dir)
    }
}
