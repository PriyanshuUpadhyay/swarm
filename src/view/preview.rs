use serde::{Deserialize, Serialize};

/// A display copy. `id` is the source locator for the full-output route.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct TextPreview {
    pub id: String,
    pub text: String,
    #[serde(skip_serializing_if = "super::is_false")]
    pub is_truncated: bool,
    pub full_lines: usize,
    pub full_bytes: usize,
    pub hidden_lines: usize,
}

// The web size guard counts Unicode scalars and always cuts on a UTF-8 boundary.
pub(crate) fn prefix_end(text: &str, limit: usize) -> usize {
    text.char_indices()
        .nth(limit)
        .map_or(text.len(), |(index, _)| index)
}

impl TextPreview {
    pub fn new(source: &str, line_limit: usize) -> Self {
        let prefix = &source[..prefix_end(source, 12_000)];
        let mut lines: Vec<_> = prefix.split('\n').collect();
        if lines.len() > 1 && lines.last() == Some(&"") {
            lines.pop();
        }
        let kept = line_limit.min(lines.len());
        let text = if kept < lines.len() {
            lines[..kept].join("\n")
        } else {
            prefix.into()
        };
        let full_lines = source.bytes().filter(|byte| *byte == b'\n').count()
            + usize::from(!source.ends_with('\n'));
        Self {
            id: String::new(),
            text,
            is_truncated: prefix.len() < source.len() || kept < lines.len(),
            full_lines,
            full_bytes: source.len(),
            hidden_lines: full_lines.saturating_sub(kept),
        }
    }
}

pub fn text_chunks(source: &str, limit: usize) -> Vec<String> {
    assert!(limit > 0);
    let mut pieces = Vec::new();
    let mut current = String::new();
    for line in source.split_inclusive('\n') {
        if !current.is_empty()
            && prefix_end(&(current.clone() + line), limit) < current.len() + line.len()
        {
            pieces.push(std::mem::take(&mut current));
        }
        let mut rest = line;
        while prefix_end(rest, limit) < rest.len() {
            let end = prefix_end(rest, limit);
            pieces.push(rest[..end].into());
            rest = &rest[end..];
        }
        current.push_str(rest);
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: i64,
    pub old_lines: i64,
    pub new_start: i64,
    pub new_lines: i64,
    pub lines: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Diff {
    #[serde(default)]
    pub tool_call_id: String,
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DiffPreview {
    pub id: String,
    pub path: String,
    pub patch: String,
    pub full_lines: usize,
    pub full_bytes: usize,
    pub omitted_lines: usize,
    pub shortened_lines: usize,
    pub added: usize,
    pub removed: usize,
}
impl DiffPreview {
    pub fn new(diff: &Diff) -> Self {
        let full = diff_patch(diff, true).0;
        let (patch, omitted_lines, shortened_lines) = diff_patch(diff, false);
        let (added, removed) = diff_counts(diff);
        Self {
            id: String::new(),
            path: diff.path.clone(),
            patch,
            full_lines: diff.hunks.iter().map(|hunk| hunk.lines.len()).sum(),
            full_bytes: full.len(),
            omitted_lines,
            shortened_lines,
            added,
            removed,
        }
    }
}
pub fn diff_counts(diff: &Diff) -> (usize, usize) {
    let lines = diff.hunks.iter().flat_map(|hunk| &hunk.lines);
    lines.fold((0, 0), |(added, removed), line| {
        (
            added + usize::from(line.starts_with('+')),
            removed + usize::from(line.starts_with('-')),
        )
    })
}
pub fn diff_patch(diff: &Diff, full: bool) -> (String, usize, usize) {
    let old = serde_json::to_string(&format!("a/{}", diff.path)).unwrap();
    let new = serde_json::to_string(&format!("b/{}", diff.path)).unwrap();
    let mut output = format!("diff --git {old} {new}\n--- {old}\n+++ {new}\n");
    let mut remaining = if full { usize::MAX } else { 200 };
    let mut omitted = 0;
    let mut shortened = 0;
    for hunk in &diff.hunks {
        let visible = &hunk.lines[..remaining.min(hunk.lines.len())];
        omitted += hunk.lines.len() - visible.len();
        remaining -= visible.len();
        if visible.is_empty() {
            continue;
        }
        let old_count = visible
            .iter()
            .filter(|line| line.starts_with([' ', '-']))
            .count();
        let new_count = visible
            .iter()
            .filter(|line| line.starts_with([' ', '+']))
            .count();
        output += &format!(
            "@@ -{},{old_count} +{},{new_count} @@\n",
            hunk.old_start, hunk.new_start
        );
        for line in visible {
            let end = if full {
                line.len()
            } else {
                prefix_end(line, 400)
            };
            output += &line[..end];
            if end < line.len() {
                shortened += 1;
                output += "…";
            }
            output += "\n";
        }
    }
    (output, omitted, shortened)
}
