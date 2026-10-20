//! Plan diffs and display forms.

use super::*;

/// The unified diff of one planned file.
pub(super) fn link_text(state: &LinkState) -> String {
    match state {
        LinkState::Absent => "absent\n".into(),
        LinkState::Link(target) => format!(
            "link -> {}\n",
            serde_json::Value::from(target.to_str().unwrap())
        ),
    }
}

pub fn diff(plan: &Plan) -> String {
    match plan {
        Plan::File(plan) => {
            crate::diff::unified(&plan.path.to_string_lossy(), &plan.before, &plan.after)
        }
        Plan::Link(plan) => crate::diff::unified(
            &plan.path.to_string_lossy(),
            &link_text(&plan.before),
            &link_text(&plan.after),
        ),
    }
}

/// Each conflict with its fix, and a last line that says no file was written; None without one.
pub fn conflicts_text(plans: &[Plan]) -> Option<String> {
    let conflicts: Vec<_> = plans.iter().flat_map(Plan::conflicts).collect();
    if conflicts.is_empty() {
        return None;
    }
    let mut text: String = conflicts
        .iter()
        .map(|conflict| {
            format!(
                "conflict: {} {}\n  found:  {}\n  wanted: {}\n  fix:    {}\n\n",
                conflict.file, conflict.entry, conflict.found, conflict.wanted, conflict.fix
            )
        })
        .collect();
    let plural = if conflicts.len() == 1 { "" } else { "s" };
    text += &format!("{} conflict{plural}. No file written.", conflicts.len());
    Some(text)
}

/// Each diff, then the conflicts, or else `unchanged` when no file changes, or else the line that
/// names `apply`, the command that writes the plan.
pub fn plan_text(plans: &[Plan], apply: &str, unchanged: &str) -> String {
    let diffs: String = plans.iter().map(diff).collect();
    let last = match conflicts_text(plans) {
        Some(conflicts) => format!("\n{conflicts}"),
        None if diffs.is_empty() => unchanged.into(),
        None => format!("\nPlan only. No file written. Run `{apply}` to apply."),
    };
    format!("{diffs}{last}\n")
}

/// The plan for the app: the digest that apply checks, each file that changes with its diff, and
/// each conflict.
pub fn plan_json(plans: &[Plan]) -> serde_json::Value {
    serde_json::json!({
        "digest": digest(plans),
        "files": plans
            .iter()
            .filter(|plan| plan.changes())
            .map(|plan| serde_json::json!({"path": plan.path().to_string_lossy(), "diff": diff(plan)}))
            .collect::<Vec<_>>(),
        "conflicts": plans.iter().flat_map(Plan::conflicts).collect::<Vec<_>>(),
    })
}
