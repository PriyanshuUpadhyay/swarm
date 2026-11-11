//! Checklist and plan rows: the to-do list and the plan that an agent CLI shows.

use super::{Row, RowKind};
use serde::Serialize;
use serde_json::Value;

/// One step of a checklist. `status` is `pending`, `in_progress`, or `completed`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TodoItem {
    pub text: String,
    pub status: String,
    /// What the step is doing now ("Editing"), shown while it runs.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub active_text: String,
}

/// Claude `TaskCreate` and `TaskUpdate` change one task; the builder replays them in order.
pub fn is_task_change(name: &str) -> bool {
    matches!(name, "TaskCreate" | "TaskUpdate")
}

fn status(value: &Value) -> Option<&'static str> {
    match value.as_str()? {
        "pending" => Some("pending"),
        "in_progress" => Some("in_progress"),
        "completed" => Some("completed"),
        _ => None,
    }
}

fn text_of(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").trim().into()
}

/// The row for a to-do or plan tool call, or None when the call is another tool or its input
/// does not parse, so the generic tool row stays.
pub fn tool_row(name: &str, input: &Value, id: String) -> Option<Row> {
    let mut row = match name {
        "TodoWrite" | "update_plan" => {
            let (list, text_key, active_key) = if name == "TodoWrite" {
                (&input["todos"], "content", "activeForm")
            } else {
                (&input["plan"], "step", "")
            };
            let todo = list
                .as_array()?
                .iter()
                .filter_map(|entry| {
                    let text = text_of(entry, text_key);
                    Some(TodoItem {
                        status: status(&entry["status"])?.into(),
                        active_text: text_of(entry, active_key),
                        text: (!text.is_empty()).then_some(text)?,
                    })
                })
                .collect();
            let mut row = Row::new(RowKind::TodoList, String::new(), id);
            row.todo = todo;
            row
        }
        "TaskCreate" | "TaskUpdate" => Row::new(RowKind::TodoList, String::new(), id),
        "ExitPlanMode" => {
            let plan = text_of(input, "plan");
            if plan.is_empty() {
                return None;
            }
            Row::new(RowKind::ProposedPlan, plan, id)
        }
        _ => return None,
    };
    if row.kind == RowKind::TodoList {
        row.text = summary(&row.todo);
    }
    Some(row)
}

/// "2 of 5 done", the header text of a checklist.
pub fn summary(todo: &[TodoItem]) -> String {
    let done = todo
        .iter()
        .filter(|item| item.status == "completed")
        .count();
    format!("{done} of {} done", todo.len())
}

/// A `TaskCreate` or `TaskUpdate` applied to the tasks so far. `created_id` is the id from the
/// create's result, and None while the result is missing.
pub fn apply_task(
    tasks: &mut Vec<(String, TodoItem)>,
    name: &str,
    input: &Value,
    call_id: &str,
    created_id: Option<&str>,
) {
    if name == "TaskCreate" {
        let text = text_of(input, "subject");
        if !text.is_empty() {
            tasks.push((
                created_id.unwrap_or(call_id).into(),
                TodoItem {
                    text,
                    status: "pending".into(),
                    active_text: text_of(input, "activeForm"),
                },
            ));
        }
        return;
    }
    let id = input["taskId"].as_str().unwrap_or("");
    let Some(at) = tasks.iter().position(|(task, _)| task == id) else {
        return;
    };
    if input["status"].as_str() == Some("deleted") {
        tasks.remove(at);
        return;
    }
    let item = &mut tasks[at].1;
    if let Some(next) = status(&input["status"]) {
        item.status = next.into();
    }
    for (key, field) in [
        ("subject", &mut item.text),
        ("activeForm", &mut item.active_text),
    ] {
        let value = text_of(input, key);
        if !value.is_empty() {
            *field = value;
        }
    }
}
