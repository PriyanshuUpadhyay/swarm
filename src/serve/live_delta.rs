//! Key-bearing server frames for canonical row deltas.

use super::*;
use swarm::view::transcript::reader_rows::RowDelta;

pub(super) fn row_frame(key: &str, delta: RowDelta) -> Frame {
    let display = view::markdown::row_displays(&delta.upsert);
    let body = view::markdown::row_bodies(&delta.upsert, &display);
    let patch = view::patch_wire::row_patches(&delta.upsert);
    let find = view::find_wire::row_finds(&delta.upsert);
    let images = view::image_paths::row_images(&delta.upsert);
    let mut payload = json!({
        "key": key,
        "upsert": delta.upsert,
        "remove": delta.remove,
        "end_offset": delta.end_offset,
        "display": display,
        "body": body,
        "patch": patch,
        "find": find,
        "images": images,
    });
    if let Some(usage) = delta.usage {
        payload["usage"] = json!(usage);
    }
    Frame::new("rows", payload, Some(key))
}

pub(super) fn live_frame(
    key: &str,
    shown: view::live_shown::Shown,
    display: Option<view::markdown::display::Display>,
) -> Frame {
    let live_body = view::markdown::display::live_body(display.as_ref());
    Frame::new(
        "live",
        json!({
            "key": key,
            "text": shown.raw.text,
            "thinking": shown.raw.thinking,
            "display": display,
            "live_body": live_body,
            "ended": shown.ended,
            "hide_text": shown.hide_text,
            "hide_thinking": shown.hide_thinking,
        }),
        Some(key),
    )
}

#[cfg(test)]
#[path = "live_delta/tests.rs"]
// Run locally with `cargo test --features serve performance_tests -- --ignored --test-threads=1`.
mod performance_tests;
