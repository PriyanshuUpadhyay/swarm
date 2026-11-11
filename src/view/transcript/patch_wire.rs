//! Patch display values beside canonical rows, without changing row JSON.

use super::display_patch::{self, PatchKind, SplitRow};
use super::{Row, TextPreview};
use serde_json::{Map, Number, Value, json};
use std::collections::BTreeMap;

fn kind(kind: PatchKind) -> &'static str {
    match kind {
        PatchKind::Add => "add",
        PatchKind::Del => "del",
        PatchKind::Ctx => "ctx",
        PatchKind::Hunk => "hunk",
        PatchKind::Meta => "meta",
    }
}

fn number(value: f64) -> Option<Number> {
    if !value.is_finite() {
        None
    } else if value.fract() == 0.0 && (0.0..18_446_744_073_709_551_616.0).contains(&value) {
        // Preserve integer JSON below 2^64; the browser still reads binary64.
        Some(Number::from(value as u64))
    } else {
        Number::from_f64(value)
    }
}

/// Build the diff layout or text colour data for a preview or full output.
pub fn text_patch(text: &str, diff: bool) -> Option<Value> {
    if !diff && !text.split('\n').any(|line| line.starts_with("@@ ")) {
        return None;
    }
    let utf16: Vec<_> = text.encode_utf16().collect();
    Some(patch_value(&utf16, diff))
}

fn line_value(line: &display_patch::PatchLine<'_>) -> Value {
    let mut value = Map::new();
    value.insert("kind".into(), json!(kind(line.kind)));
    value.insert("text".into(), json!(String::from_utf16_lossy(line.text)));
    for (key, n) in [("old", line.old), ("new", line.new)] {
        if let Some(n) = n.and_then(number) {
            value.insert(key.into(), Value::Number(n));
        }
    }
    Value::Object(value)
}

/// Encode only the displayed Changes lines from the capped preview.
pub fn changes_patch(text: &str) -> Value {
    let utf16: Vec<_> = text.encode_utf16().collect();
    let lines: Vec<_> = display_patch::comment_lines(&utf16)
        .iter()
        .map(|line| {
            let mut value = line_value(line);
            if let Some(quote) = display_patch::quote(line) {
                value["quote"] = json!(String::from_utf16_lossy(&quote));
            }
            value
        })
        .collect();
    json!({"lines": lines})
}

fn patch_value(utf16: &[u16], diff: bool) -> Value {
    if !diff {
        let lines: Vec<_> = utf16.split(|&c| c == 10).collect();
        let kinds: Vec<_> = display_patch::patch_kinds(&lines)
            .into_iter()
            .map(kind)
            .collect();
        return json!({"kinds": kinds});
    }
    let parsed = display_patch::parse_patch(utf16);
    let lines: Vec<_> = parsed.iter().map(line_value).collect();
    let mut hunks = parsed
        .iter()
        .enumerate()
        .filter(|(_, line)| line.kind == PatchKind::Hunk);
    let split: Vec<_> = display_patch::split_patch(&parsed)
        .into_iter()
        .filter_map(|row| match row {
            SplitRow::Hunk { text } => hunks
                .find(|(_, line)| line.text == text)
                .map(|(index, _)| json!({"h": index})),
            SplitRow::Pair { left, right } => Some(json!({"l": left, "r": right})),
        })
        .collect();
    json!({"lines": lines, "split": split})
}

fn note_text(patches: &mut BTreeMap<String, Value>, preview: &TextPreview) {
    let utf16: Vec<_> = preview.text.encode_utf16().collect();
    if display_patch::is_patch(&utf16) {
        patches.insert(preview.id.clone(), patch_value(&utf16, false));
    }
}

/// Build entries for every diff and patch-text preview, including fold steps.
pub fn row_patches(rows: &[Row]) -> BTreeMap<String, Value> {
    let mut patches = BTreeMap::new();
    let mut pending: Vec<_> = rows.iter().rev().collect();
    while let Some(row) = pending.pop() {
        pending.extend(row.steps.iter().rev());
        for diff in row
            .diff
            .iter()
            .chain(row.tool.iter().flat_map(|tool| &tool.diffs))
        {
            if let Some(entry) = text_patch(&diff.patch, true) {
                patches.insert(diff.id.clone(), entry);
            }
        }
        for preview in row
            .output
            .iter()
            .chain(row.tool.iter().filter_map(|tool| tool.output.as_ref()))
            .chain(row.shell.iter().map(|shell| &shell.output))
            .chain(
                row.command
                    .iter()
                    .filter_map(|command| command.output.as_ref()),
            )
        {
            note_text(&mut patches, preview);
        }
    }
    patches
}
