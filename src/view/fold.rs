use super::ToolState;
use super::transcript::{Row, RowKind};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NameCount {
    pub name: String,
    pub count: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FoldSummary {
    pub commands: usize,
    pub waits: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub other_tools: Vec<NameCount>,
    pub rings: usize,
    pub failed: usize,
    pub interrupted: usize,
    pub unreported: usize,
    #[serde(skip_serializing_if = "super::is_false")]
    pub is_running: bool,
    pub latest_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
    pub state: ToolState,
    pub text: String,
}
/// A row that a live reader chunk keeps with its neighbours, so a chunk always holds a whole group.
pub fn is_step(row: &Row) -> bool {
    row.tool.is_some()
        || (row.system_kind.as_deref() == Some("swarm_ring") && !row.starts_turn)
        || row.kind == RowKind::Thought
        || row.collapsed
}
pub fn fold_expanded(
    rows: &[Row],
    overrides: &HashMap<String, bool>,
    shown: &HashSet<String>,
) -> bool {
    rows.iter()
        .find_map(|row| overrides.get(&format!("fold:{}", row.id)).copied())
        .unwrap_or_else(|| {
            rows.iter().any(|row| {
                row.tool
                    .as_ref()
                    .is_some_and(|tool| tool.state == ToolState::Failed)
            }) || rows
                .iter()
                .filter(|row| !row.collapsed && shown.contains(&row.id))
                .count()
                >= 2
        })
}
// shortcut: folds hold at most 100 steps; raise this only after checking live frame cost.
pub const FOLD_STEP_LIMIT: usize = 100;

/// Groups the tool calls that the agent's own CLI draws as one group (ADR 0157): consecutive calls
/// with the same `group` mark. Thinking and hidden records that arrive while a group is open go
/// into it, as Claude Code and Codex do. Every other row ends the group, and an unmarked call is
/// never grouped, so a pi chat has no groups.
pub fn fold_rows(rows: Vec<Row>, shown: &HashSet<String>) -> Vec<Row> {
    fn flush(run: &mut Vec<Row>, items: &mut Vec<Row>, shown: &HashSet<String>) {
        let Some(first) = run.first() else {
            return;
        };
        let id = format!("fold:{}", first.id);
        let summary = summary(run);
        let mut fold = Row::new(RowKind::Fold, summary.text.clone(), id);
        fold.order = first.order;
        fold.default_open = fold_expanded(run, &HashMap::new(), shown);
        fold.source_ids = run.iter().flat_map(|row| row.source_ids.clone()).collect();
        fold.fold = Some(summary);
        fold.steps = std::mem::take(run);
        items.push(fold);
    }
    let mut items = Vec::new();
    let mut run: Vec<Row> = Vec::new();
    for row in rows {
        let open = run.first().map(|first| first.group.as_str());
        if row.tool.is_some() && !row.group.is_empty() {
            if open != Some(row.group.as_str()) || run.len() == FOLD_STEP_LIMIT {
                flush(&mut run, &mut items, shown);
            }
            run.push(row);
        } else if open.is_some()
            && (row.kind == RowKind::Thought || row.collapsed)
            && run.len() < FOLD_STEP_LIMIT
        {
            run.push(row);
        } else {
            flush(&mut run, &mut items, shown);
            items.push(row);
        }
    }
    flush(&mut run, &mut items, shown);
    items
}
pub fn summary(rows: &[Row]) -> FoldSummary {
    let mut result = FoldSummary {
        commands: 0,
        waits: 0,
        other_tools: Vec::new(),
        rings: 0,
        failed: 0,
        interrupted: 0,
        unreported: 0,
        is_running: false,
        latest_title: String::new(),
        duration_seconds: Some(0.0),
        state: ToolState::Finished,
        text: String::new(),
    };
    for row in rows.iter().filter(|row| !row.collapsed) {
        if row.system_kind.as_deref() == Some("swarm_ring") {
            result.rings += 1;
        }
        let Some(tool) = &row.tool else {
            continue;
        };
        if tool.name == "sleep" {
            result.waits += 1;
        } else if tool.command.is_some() {
            result.commands += 1;
        } else if let Some(count) = result
            .other_tools
            .iter_mut()
            .find(|count| count.name == tool.name)
        {
            count.count += 1;
        } else {
            result.other_tools.push(NameCount {
                name: tool.name.clone(),
                count: 1,
            });
        }
        match tool.state {
            ToolState::Failed => result.failed += 1,
            ToolState::Waiting => {
                result.is_running = true;
                result.latest_title = if tool.header_title.is_empty() {
                    tool.name.clone()
                } else {
                    tool.header_title.clone()
                };
            }
            ToolState::Interrupted => result.interrupted += 1,
            ToolState::Unreported => result.unreported += 1,
            ToolState::Finished => (),
        }
        result.duration_seconds = result
            .duration_seconds
            .zip(tool.duration_seconds)
            .map(|(sum, duration)| sum + duration);
    }
    result.state = if result.failed > 0 {
        ToolState::Failed
    } else if result.is_running {
        ToolState::Waiting
    } else if result.interrupted > 0 {
        ToolState::Interrupted
    } else if result.unreported > 0 {
        ToolState::Unreported
    } else {
        ToolState::Finished
    };
    // Each group reads as the agent's own CLI labels it.
    let mut parts = vec![match rows.first().map_or("", |row| row.group.as_str()) {
        "explore" if result.is_running => "Exploring".to_owned(),
        "explore" => "Explored".to_owned(),
        _ => read_search_label(rows, result.is_running),
    }];
    for (count, suffix) in [
        (result.interrupted, "interrupted"),
        (result.unreported, "no result"),
        (result.failed, "failed"),
    ] {
        if count > 0 {
            parts.push(format!("{count} {suffix}"));
        }
    }
    result.text = parts.join(" · ");
    result
}

/// Claude Code's label: "Searched for 2 patterns, read 3 files, listed 1 directory"; while the
/// group runs, "Searching for", "reading", and "listing". A file read twice counts once.
fn read_search_label(rows: &[Row], running: bool) -> String {
    let tools: Vec<_> = rows
        .iter()
        .filter(|row| !row.collapsed && row.tool.is_some())
        .collect();
    let count = |part: &str| tools.iter().filter(|row| row.group_part == part).count();
    let mut files = HashSet::new();
    let reads = tools
        .iter()
        .filter(|row| row.group_part == "read")
        .filter(
            |row| match row.tool.as_ref().and_then(|tool| tool.path.as_deref()) {
                Some(path) => files.insert(path),
                None => true,
            },
        )
        .count();
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut parts = Vec::new();
    let searches = count("search");
    if searches > 0 {
        let verb = if running {
            "searching for"
        } else {
            "searched for"
        };
        parts.push(format!(
            "{verb} {}",
            plural(searches, "pattern", "patterns")
        ));
    }
    if reads > 0 {
        let verb = if running { "reading" } else { "read" };
        parts.push(format!("{verb} {}", plural(reads, "file", "files")));
    }
    let lists = count("list");
    if lists > 0 {
        let verb = if running { "listing" } else { "listed" };
        parts.push(format!(
            "{verb} {}",
            plural(lists, "directory", "directories")
        ));
    }
    let text = parts.join(", ");
    let mut characters = text.chars();
    characters.next().map_or(text.clone(), |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}
