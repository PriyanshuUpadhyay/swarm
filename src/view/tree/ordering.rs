// Order workspace rows and count agent statuses.
use super::{AgentStatus, Workspace};
use serde::Serialize;
use std::collections::BTreeMap;

pub fn order_workspaces(
    workspaces: &mut [Workspace],
    order: &[String],
    main: Option<&str>,
    hub: &str,
) {
    let group = |w: &Workspace| {
        if w.removed {
            2
        } else if w.path == hub && Some(w.path.as_str()) != main {
            1
        } else {
            0
        }
    };
    workspaces.sort_by(|a, b| {
        order
            .iter()
            .position(|p| p == &a.path)
            .unwrap_or(usize::MAX)
            .cmp(
                &order
                    .iter()
                    .position(|p| p == &b.path)
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| group(a).cmp(&group(b)))
            .then_with(|| (Some(b.path.as_str()) == main).cmp(&(Some(a.path.as_str()) == main)))
            .then_with(|| natural_cmp(&a.name, &b.name))
            .then_with(|| a.path.cmp(&b.path))
    });
}

pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut left = String::new();
                let mut right = String::new();
                while a.peek().is_some_and(char::is_ascii_digit) {
                    left.push(a.next().unwrap());
                }
                while b.peek().is_some_and(char::is_ascii_digit) {
                    right.push(b.next().unwrap());
                }
                let left = left.trim_start_matches('0');
                let right = right.trim_start_matches('0');
                let order = left.len().cmp(&right.len()).then_with(|| left.cmp(right));
                if !order.is_eq() {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                a.next();
                b.next();
                let order = x.cmp(&y);
                if !order.is_eq() {
                    return order;
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatusCount {
    pub status: AgentStatus,
    pub count: usize,
}
/// Counts only live agents, so a closed chat shows no badge (ADR 0083).
pub fn status_counts(statuses: impl IntoIterator<Item = AgentStatus>) -> Vec<StatusCount> {
    let mut counts = BTreeMap::new();
    for status in statuses.into_iter().filter(|s| *s != AgentStatus::Ended) {
        *counts.entry(status).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .rev()
        .map(|(status, count)| StatusCount { status, count })
        .collect()
}
