//! Recount agent-written fenced patches without changing their copy source.

use super::super::patch_wire;
use serde_json::{Value, json};

struct Hunk {
    index: usize,
    from: f64,
    to: f64,
    olds: usize,
    news: usize,
    title: String,
}

impl Hunk {
    fn close(&self, lines: &mut [String]) {
        lines[self.index] = format!(
            "@@ -{},{} +{},{} @@{}",
            self.from, self.olds, self.to, self.news, self.title
        );
    }
}

fn starts(line: &str) -> Option<(f64, f64, &str)> {
    let (range, title) = line.strip_prefix("@@ -")?.split_once(" @@")?;
    let (old, new) = range.split_once(" +")?;
    let start = |range: &str| {
        let (start, count) = range
            .split_once(',')
            .map_or((range, None), |(a, b)| (a, Some(b)));
        if start.is_empty()
            || !start.bytes().all(|c| c.is_ascii_digit())
            || count.is_some_and(|n| n.is_empty() || !n.bytes().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        start.parse().ok()
    };
    Some((start(old)?, start(new)?, title))
}

fn file_start(lines: &[&str], index: usize) -> bool {
    lines[index].starts_with("diff --git")
        || (lines[index].starts_with("--- ")
            && lines
                .get(index + 1)
                .is_some_and(|line| line.starts_with("+++ ")))
}

/// Each file is parsed apart, so a file header cannot become a deletion in the previous hunk.
pub(super) fn patch(text: &str) -> Value {
    let lines: Vec<_> = text
        .strip_suffix('\n')
        .unwrap_or(text)
        .split('\n')
        .collect();
    let mut sections = Vec::new();
    let mut out = Vec::<String>::new();
    let mut hunk: Option<Hunk> = None;
    let (mut from, mut to) = (1.0, 1.0);
    for (index, &line) in lines.iter().enumerate() {
        if file_start(&lines, index) {
            if let Some(hunk) = hunk.take() {
                hunk.close(&mut out);
            }
            if !out.is_empty() {
                sections.push(std::mem::take(&mut out));
            }
            (from, to) = (1.0, 1.0);
            out.push(line.to_owned());
            continue;
        }
        if line.starts_with("@@") {
            if let Some(previous) = hunk.take() {
                previous.close(&mut out);
                from = previous.from + previous.olds as f64;
                to = previous.to + previous.news as f64;
            }
            let (old, new, title) = starts(line).unwrap_or((from, to, ""));
            hunk = Some(Hunk {
                index: out.len(),
                from: old,
                to: new,
                olds: 0,
                news: 0,
                title: title.to_owned(),
            });
            out.push(String::new());
            continue;
        }
        if hunk.is_none() {
            if line.starts_with("+++ ") || !line.starts_with(['-', '+']) {
                out.push(line.to_owned());
                continue;
            }
            hunk = Some(Hunk {
                index: out.len(),
                from,
                to,
                olds: 0,
                news: 0,
                title: String::new(),
            });
            out.push(String::new());
        }
        out.push(line.to_owned());
        let hunk = hunk.as_mut().unwrap();
        if line.starts_with('-') {
            hunk.olds += 1;
        } else if line.starts_with('+') {
            hunk.news += 1;
        } else if !line.starts_with('\\') {
            hunk.olds += 1;
            hunk.news += 1;
        }
    }
    if let Some(hunk) = hunk {
        hunk.close(&mut out);
    }
    sections.push(out);
    let mut lines = Vec::new();
    let mut split = Vec::new();
    for section in sections {
        let value = patch_wire::text_patch(&section.join("\n"), true).unwrap();
        let offset = lines.len();
        for row in value["split"].as_array().unwrap() {
            let mut row = row.clone();
            for key in ["h", "l", "r"] {
                if let Some(index) = row[key].as_u64() {
                    row[key] = json!(index + offset as u64);
                }
            }
            split.push(row);
        }
        lines.extend(value["lines"].as_array().unwrap().iter().cloned());
    }
    json!({"lines": lines, "split": split})
}
