//! Matching over the browser's loaded snapshot. Offsets count UTF-16 units in lowercase text.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
pub struct Request {
    pub version: u8,
    query: String,
    rows: Vec<FindRow>,
    find: Option<BTreeMap<String, Vec<Part>>>,
    full: Option<BTreeMap<String, String>>,
}

// This request view keeps arbitrary own fields for FindPart paths; canonical Row stays output-only.
#[derive(Deserialize)]
#[serde(try_from = "Value")]
struct FindRow(Value);

impl TryFrom<Value> for FindRow {
    type Error = &'static str;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        fn valid(row: &Value) -> bool {
            row.is_object()
                && row.get("id").is_some_and(Value::is_string)
                && row
                    .get("collapsed")
                    .is_none_or(|v| v.is_null() || v.is_boolean())
                && row.get("steps").is_none_or(|v| {
                    v.is_null() || v.as_array().is_some_and(|steps| steps.iter().all(valid))
                })
        }
        valid(&value).then_some(Self(value)).ok_or("invalid row")
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Part {
    Literal { t: String },
    Field { f: String, p: Option<String> },
}

#[derive(Serialize)]
pub struct Reply {
    version: u8,
    matches: Vec<Match>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Match {
    id: String,
    fold: Option<String>,
    offset: usize,
    occurrence: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_occurrence: Option<usize>,
}

fn part_text<'a>(row: &'a Value, part: &'a Part) -> &'a str {
    match part {
        Part::Literal { t } => t,
        Part::Field { f, .. } => {
            let mut value = row;
            for key in f.split('.') {
                let next = match value {
                    Value::Object(fields) => fields.get(key),
                    Value::Array(items) => key.parse::<usize>().ok().and_then(|index| {
                        (index.to_string() == key)
                            .then(|| items.get(index))
                            .flatten()
                    }),
                    _ => None,
                };
                let Some(next) = next else { return "" };
                value = next;
            }
            value.as_str().unwrap_or("")
        }
    }
}

// ECMAScript WhiteSpace + LineTerminator, including FEFF but excluding Rust's NEL whitespace.
fn js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

pub fn find(request: Request) -> Reply {
    let query = request.query.trim_matches(js_space).to_lowercase();
    let mut matches = Vec::new();
    if !query.is_empty() {
        let mut pending: Vec<_> = request.rows.iter().rev().map(|r| (&r.0, None)).collect();
        while let Some((row, parent)) = pending.pop() {
            if row.get("collapsed").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            // FindRow validated every ID and child before matching.
            let id = row["id"].as_str().unwrap();
            if let Some(steps) = row.get("steps").and_then(Value::as_array) {
                pending.extend(steps.iter().rev().map(|step| (step, Some(id))));
            }
            let mut occurrence = 0;
            for part in request
                .find
                .as_ref()
                .and_then(|find| find.get(id))
                .into_iter()
                .flatten()
            {
                let preview = match part {
                    Part::Field { p, .. } => p.as_deref().filter(|p| !p.is_empty()),
                    _ => None,
                };
                let full = preview.and_then(|p| request.full.as_ref()?.get(p));
                let value = full
                    .map(String::as_str)
                    .unwrap_or_else(|| part_text(row, part));
                // str::to_lowercase uses full Unicode mappings and contextual final sigma.
                let text = value.to_lowercase();
                let (mut cursor, mut scanned, mut offset, mut line, mut on_line) = (0, 0, 0, 0, 0);
                while let Some(relative) = text[cursor..].find(&query) {
                    let start = cursor + relative;
                    let before = &text[scanned..start];
                    offset += before.encode_utf16().count();
                    let newlines = before.bytes().filter(|b| *b == b'\n').count();
                    if newlines > 0 {
                        line += newlines;
                        on_line = 0;
                    }
                    matches.push(Match {
                        id: id.to_owned(),
                        fold: parent.map(str::to_owned),
                        offset,
                        occurrence,
                        preview: preview.map(str::to_owned),
                        line: preview.map(|_| line),
                        line_occurrence: preview.map(|_| on_line),
                    });
                    occurrence += 1;
                    on_line += 1;
                    scanned = start;
                    cursor = start + query.len();
                }
            }
        }
    }
    Reply {
        version: 1,
        matches,
    }
}
