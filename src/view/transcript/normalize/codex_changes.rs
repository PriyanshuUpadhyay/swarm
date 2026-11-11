//! Saved Codex file changes join the patch call; no file is read or changed here.

use super::claude_json::Object;
use super::codex_facts::Fact;
use super::{SnapshotError, Work};
use serde_json::{Value, json};

pub(super) fn append(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    id_field: &str,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let id = payload.text(id_field);
    let status = payload.text("status");
    let failed = payload.boolean("success") == Some(false)
        || matches!(status.as_str(), "failed" | "declined");
    fact.push("tool_call_update", json!({
        "tool_call_id": id, "status": if failed { "failed" } else { "completed" },
        "content": ([payload.text("stdout"), payload.text("stderr")].into_iter().filter(|text| !text.is_empty()).collect::<Vec<_>>().join("\n")),
    }));
    if let Some(changes) = payload.object("changes") {
        for (path, raw) in changes.fields() {
            work.check()?;
            let Some(change) = Object::parse(raw) else {
                fact.push_unknown();
                continue;
            };
            let hunks = match change.text("type").as_str() {
                "add" | "delete" => {
                    let add = change.text("type") == "add";
                    change.optional_text("content").map(|text| {
                        let lines: Vec<_> = text.lines().map(|line| format!("{}{line}", if add { '+' } else { '-' })).collect();
                        vec![json!({"old_start":if add {0} else {1}, "old_lines":if add {0} else {lines.len()},
                            "new_start":if add {1} else {0}, "new_lines":if add {lines.len()} else {0}, "lines":lines})]
                    })
                }
                "update" => unified(&change.text("unified_diff"), work)?,
                _ => None,
            };
            if let Some(hunks) = hunks {
                fact.push(
                    "tool_diff",
                    json!({"tool_call_id":id,"path":path,"hunks":hunks}),
                );
            } else {
                fact.push_unknown();
            }
        }
    }
    Ok(())
}

fn range(text: &str, prefix: char) -> Option<(i64, i64)> {
    let text = text.strip_prefix(prefix)?;
    let (start, count) = text.split_once(',').unwrap_or((text, "1"));
    let (start, count) = (start.parse::<i64>().ok()?, count.parse::<i64>().ok()?);
    (start >= 0 && count >= 0).then_some((start, count))
}

fn unified(text: &str, work: &mut dyn Work) -> Result<Option<Vec<Value>>, SnapshotError> {
    let mut hunks = Vec::new();
    let mut hunk: Option<(i64, i64, i64, i64, Vec<String>)> = None;
    for line in text.lines() {
        work.check()?;
        if let Some(header) = line.strip_prefix("@@ ") {
            if let Some(previous) = hunk.take() {
                let Some(value) = finish(previous) else {
                    return Ok(None);
                };
                hunks.push(value);
            }
            let mut fields = header.split_whitespace();
            let parsed = fields
                .next()
                .and_then(|old| range(old, '-'))
                .zip(fields.next().and_then(|new| range(new, '+')));
            let Some(((old_start, old_lines), (new_start, new_lines))) = parsed else {
                return Ok(None);
            };
            if fields.next() != Some("@@") {
                return Ok(None);
            }
            hunk = Some((old_start, old_lines, new_start, new_lines, Vec::new()));
        } else if let Some((_, _, _, _, lines)) = &mut hunk {
            if !line.starts_with([' ', '+', '-']) && line != "\\ No newline at end of file" {
                return Ok(None);
            }
            lines.push(line.into());
        } else if !["diff ", "--- ", "+++ ", "index "]
            .iter()
            .any(|prefix| line.starts_with(prefix))
            && !line.is_empty()
        {
            return Ok(None);
        }
    }
    if let Some(previous) = hunk {
        let Some(value) = finish(previous) else {
            return Ok(None);
        };
        hunks.push(value);
    }
    Ok((!hunks.is_empty()).then_some(hunks))
}

fn finish(
    (old_start, old_lines, new_start, new_lines, lines): (i64, i64, i64, i64, Vec<String>),
) -> Option<Value> {
    if lines
        .iter()
        .filter(|line| line.starts_with([' ', '-']))
        .count()
        != old_lines as usize
        || lines
            .iter()
            .filter(|line| line.starts_with([' ', '+']))
            .count()
            != new_lines as usize
    {
        return None;
    }
    Some(
        json!({"old_start":old_start,"old_lines":old_lines,"new_start":new_start,"new_lines":new_lines,"lines":lines}),
    )
}
