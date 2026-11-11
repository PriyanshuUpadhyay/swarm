//! Frozen OLD bytes, split into JSON hex strings; manifest hashes name the source bytes.
use serde_json::{Value, json};
use std::path::Path;

fn manifest(root: &Path) -> Value {
    let parts: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let mut all = serde_json::Map::new();
    for name in parts {
        let value: Value =
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
        all.extend(value.as_object().unwrap().clone());
    }
    Value::Object(all)
}

pub fn bytes(key: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/s9b");
    let manifest = manifest(&root);
    let mut bytes = Vec::new();
    for part in manifest[key]["parts"].as_array().expect(key) {
        let hex: String =
            serde_json::from_slice(&std::fs::read(root.join(part.as_str().unwrap())).unwrap())
                .unwrap();
        bytes.extend(
            (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()),
        );
    }
    bytes
}

pub fn parse(name: &str, text: &str) -> Value {
    if name.ends_with(".json") {
        let mut value: Value = serde_json::from_str(text).unwrap();
        for key in ["rows", "raw"] {
            if let Some(frames) = value[key].as_array_mut() {
                frames.retain(|frame| frame["event"] != "ping");
            }
        }
        return value;
    }
    let (locator, text) = if name.starts_with("output-") {
        let (id, rest) = text.split_once('\n').unwrap();
        (Some(id), rest)
    } else {
        (None, text)
    };
    let (status, body) = text.split_once('\n').unwrap();
    let body: Value = serde_json::from_str(body).unwrap();
    let mut value = json!({"status":status.parse::<u16>().unwrap(), "body":body});
    if let Some(id) = locator {
        value["locator"] = json!(id);
    }
    value
}

pub fn expected(prefixes: &[&str]) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/s9b");
    let manifest = manifest(&root);
    let images: Value = serde_json::from_str(include_str!("../fixtures/s9b/images.json")).unwrap();
    let mut result = serde_json::Map::new();
    for key in manifest.as_object().unwrap().keys() {
        if let Some(relative) = key.strip_prefix("out-1/")
            && (prefixes.is_empty() || prefixes.iter().any(|p| relative.starts_with(p)))
        {
            let name = relative.rsplit('/').next().unwrap();
            let mut value = parse(name, &String::from_utf8(bytes(key)).unwrap());
            if let Some(additions) = images.get(relative) {
                if let Some(paths) = additions.get("page") {
                    value["body"]["images"] = paths.clone();
                }
                if let Some(paths) = additions["rows"].as_array() {
                    let frames = value["rows"].as_array_mut().unwrap();
                    assert_eq!(frames.len(), paths.len());
                    for (frame, paths) in frames.iter_mut().zip(paths) {
                        frame["data"]["images"] = paths.clone();
                    }
                }
            }
            result.insert(relative.into(), value);
        }
    }
    Value::Object(result)
}
