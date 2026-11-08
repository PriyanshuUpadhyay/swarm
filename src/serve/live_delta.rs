//! Key-bearing server frames for canonical row deltas.

use super::*;
use swarm::view::transcript::reader_rows::RowDelta;

pub(super) fn row_frame(key: &str, delta: RowDelta) -> Frame {
    let display = view::markdown::row_displays(&delta.upsert);
    let mut payload = json!({
        "key": key,
        "upsert": delta.upsert,
        "remove": delta.remove,
        "end_offset": delta.end_offset,
        "display": display,
    });
    if let Some(usage) = delta.usage {
        payload["usage"] = json!(usage);
    }
    Frame::new("rows", payload, Some(key))
}

#[cfg(test)]
#[path = "live_delta/tests.rs"]
// Run locally with `cargo test --features serve performance_tests -- --ignored --test-threads=1`.
mod performance_tests;
