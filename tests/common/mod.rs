#![allow(dead_code)]

use serde_json::{Value, json};
use swarm::view::transcript::source::{
    EndPolicy, NativeFormat, Snapshot, SnapshotError, SnapshotReader, SourceMeta, Work,
};

pub struct Ready;

impl Work for Ready {
    fn check(&mut self) -> Result<(), SnapshotError> {
        Ok(())
    }
}

pub fn reader(format: NativeFormat, source_id: &str, path: &str) -> SnapshotReader {
    SnapshotReader::with_open_path(
        SourceMeta {
            source_id: source_id.into(),
        },
        0,
        EndPolicy::SnapshotEof,
        format,
        path,
    )
}

pub fn snapshot(format: NativeFormat, source_id: &str, path: &str, lines: &[Value]) -> Snapshot {
    let mut reader = reader(format, source_id, path);
    let text = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    reader.push(text.as_bytes(), &mut Ready).unwrap();
    reader.finish(&mut Ready).unwrap()
}

pub fn response(payload: Value) -> Value {
    json!({"type":"response_item", "payload":payload})
}

pub fn event(payload: Value) -> Value {
    json!({"type":"event_msg", "payload":payload})
}

pub fn completed(item: Value) -> Value {
    event(json!({"type":"item_completed", "item":item}))
}
