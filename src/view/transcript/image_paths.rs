//! Sent-message image paths use the original browser match rule without copying row text.
use super::{Row, RowKind};
use std::collections::{BTreeMap, HashSet};

fn js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'..='\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

pub fn image_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for segment in text.split(js_whitespace) {
        let Some(slash) = segment.find('/') else {
            continue;
        };
        // The greedy non-whitespace prefix chooses the last valid uploads suffix in this run.
        for (upload, _) in segment.rmatch_indices("/uploads/") {
            if upload <= slash + 1 {
                continue;
            }
            let suffix = &segment[upload + "/uploads/".len()..];
            let Some(uuid) = suffix.as_bytes().get(..36) else {
                continue;
            };
            if !uuid.iter().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    *byte == b'-'
                } else {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)
                }
            }) || suffix.as_bytes().get(36) != Some(&b'.')
            {
                continue;
            }
            let Some(extension) = ["png", "jpg", "gif", "webp"]
                .into_iter()
                .find(|extension| suffix[37..].starts_with(*extension))
            else {
                continue;
            };
            let end = upload + "/uploads/".len() + 37 + extension.len();
            let path = &segment[slash..end];
            if seen.insert(path) {
                paths.push(path.to_owned());
            }
            break;
        }
    }
    paths
}

pub fn row_images(rows: &[Row]) -> BTreeMap<String, Vec<String>> {
    let mut images = BTreeMap::new();
    let mut pending: Vec<_> = rows.iter().rev().collect();
    while let Some(row) = pending.pop() {
        pending.extend(row.steps.iter().rev());
        if matches!(row.kind, RowKind::User | RowKind::Queued)
            && row.shell.is_none()
            && row.command.is_none()
        {
            let paths = image_paths(&row.text);
            if !paths.is_empty() {
                images.insert(row.id.clone(), paths);
            }
        }
    }
    images
}
