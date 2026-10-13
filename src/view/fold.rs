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
pub fn is_step(row: &Row) -> bool {
    row.tool.is_some()
        || (row.system_kind.as_deref() == Some("swarm_ring") && !row.starts_turn)
        || row.kind == RowKind::Thought
        || row.hidden
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
                .filter(|row| !row.hidden && shown.contains(&row.id))
                .count()
                >= 2
        })
}
// shortcut: folds hold at most 100 steps; raise this only after checking live frame cost.
pub const FOLD_STEP_LIMIT: usize = 100;

pub fn fold_rows(rows: Vec<Row>, shown: &HashSet<String>) -> Vec<Row> {
    fn flush(run: &mut Vec<Row>, items: &mut Vec<Row>, shown: &HashSet<String>) {
        if run.iter().filter(|row| !row.hidden).count() >= 2
            && run.iter().any(|row| row.tool.is_some())
        {
            let id = format!("fold:{}", run.iter().find(|row| !row.hidden).unwrap().id);
            let summary = summary(run);
            let mut fold = Row::new(RowKind::Fold, summary.text.clone(), id);
            fold.order = run[0].order;
            fold.default_open = fold_expanded(run, &HashMap::new(), shown);
            fold.source_ids = run.iter().flat_map(|row| row.source_ids.clone()).collect();
            fold.fold = Some(summary);
            fold.steps = std::mem::take(run);
            items.push(fold);
        } else {
            items.append(run);
        }
    }
    let mut items = Vec::new();
    let mut run = Vec::new();
    for row in rows {
        if is_step(&row) {
            if run.len() == FOLD_STEP_LIMIT {
                flush(&mut run, &mut items, shown);
            }
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
    for row in rows.iter().filter(|row| !row.hidden) {
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
    let mut parts = Vec::new();
    for (count, word) in [(result.commands, "command"), (result.waits, "wait")] {
        if count > 0 {
            parts.push(format!(
                "{count} {word}{}",
                if count == 1 { "" } else { "s" }
            ));
        }
    }
    parts.extend(result.other_tools.iter().map(|count| {
        if count.count > 1 {
            format!("{} ×{}", count.name, count.count)
        } else {
            count.name.clone()
        }
    }));
    if result.rings > 0 {
        parts.push(format!(
            "{} ring{}",
            result.rings,
            if result.rings == 1 { "" } else { "s" }
        ));
    }
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
