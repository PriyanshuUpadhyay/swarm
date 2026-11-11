//! Search field choices beside canonical rows, without copying their text.

use super::{Row, RowKind, TextPreview};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn field(parts: &mut Vec<Value>, path: &str, text: &str, preview: Option<&str>) {
    if let Some(id) = preview {
        parts.push(json!({"f": path, "p": id}));
    } else if !text.is_empty() {
        parts.push(json!({"f": path}));
    }
}

fn output(parts: &mut Vec<Value>, path: &str, preview: Option<&TextPreview>) {
    if let Some(preview) = preview {
        field(parts, path, &preview.text, Some(&preview.id));
    }
}

fn row_parts(row: &Row) -> Vec<Value> {
    let mut parts = Vec::new();
    if row.system_kind.as_deref() == Some("swarm_ring") {
        parts.push(json!({"t": "New swarm message"}));
    } else if row.kind == RowKind::Fold {
        if let Some(fold) = &row.fold {
            field(&mut parts, "fold.text", &fold.text, None);
        }
    } else if let Some(tool) = &row.tool {
        field(&mut parts, "tool.name", &tool.name, None);
        field(&mut parts, "tool.header_title", &tool.header_title, None);
        if let Some(command) = &tool.command {
            field(&mut parts, "tool.command", command, None);
        }
        if let Some(path) = &tool.path {
            field(&mut parts, "tool.path", path, None);
        }
        output(&mut parts, "tool.output.text", tool.output.as_ref());
        output(&mut parts, "tool.skill_body.text", tool.skill_body.as_ref());
        for (index, diff) in tool.diffs.iter().enumerate() {
            field(
                &mut parts,
                &format!("tool.diffs.{index}.patch"),
                &diff.patch,
                Some(&diff.id),
            );
        }
    } else if let Some(shell) = &row.shell {
        if let Some(command) = &shell.command {
            field(&mut parts, "shell.command", command, None);
        }
        output(&mut parts, "shell.output.text", Some(&shell.output));
    } else if let Some(command) = &row.command {
        let text = if command.arguments.contains(&command.name) {
            command.arguments.clone()
        } else {
            format!("{} {}", command.name, command.arguments)
        };
        if !text.is_empty() {
            parts.push(json!({"t": text}));
        }
        output(
            &mut parts,
            "command.skill_body.text",
            command.skill_body.as_ref(),
        );
        output(&mut parts, "command.output.text", command.output.as_ref());
    } else if row.kind == RowKind::ToolResult {
        output(&mut parts, "output.text", row.output.as_ref());
    } else if row.kind == RowKind::Diff {
        if let Some(diff) = &row.diff {
            field(&mut parts, "diff.patch", &diff.patch, Some(&diff.id));
        }
    } else {
        field(&mut parts, "text", &row.text, None);
        for (index, item) in row.todo.iter().enumerate() {
            let key = if item.status == "in_progress" && !item.active_text.is_empty() {
                "active_text"
            } else {
                "text"
            };
            let text = if key == "active_text" {
                &item.active_text
            } else {
                &item.text
            };
            field(&mut parts, &format!("todo.{index}.{key}"), text, None);
        }
        if let Some(detail) = &row.detail {
            field(&mut parts, "detail", detail, None);
        }
        output(&mut parts, "output.text", row.output.as_ref());
        if let Some(diff) = &row.diff {
            field(&mut parts, "diff.patch", &diff.patch, Some(&diff.id));
        }
    }
    parts
}

/// Build search parts for visible rows and their fold steps in source order.
pub fn row_finds(rows: &[Row]) -> BTreeMap<String, Vec<Value>> {
    let mut finds = BTreeMap::new();
    let mut pending: Vec<_> = rows.iter().rev().collect();
    while let Some(row) = pending.pop() {
        if row.collapsed {
            continue;
        }
        pending.extend(row.steps.iter().rev());
        let parts = row_parts(row);
        if !parts.is_empty() {
            finds.insert(row.id.clone(), parts);
        }
    }
    finds
}
