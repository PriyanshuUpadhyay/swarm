use super::{CiState, MAX_SAFE_JSON_INTEGER, PrFact, PrState, WorkspaceFields};
use serde_json::Value;

pub fn github_fields(request: &Value) -> WorkspaceFields {
    let Some(number) = request["number"].as_u64() else {
        return WorkspaceFields::default();
    };
    let states: Vec<_> = request["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            c["conclusion"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or(c["state"].as_str())
                .or(c["status"].as_str())
                .unwrap_or_default()
                .to_uppercase()
        })
        .collect();

    let ci = if states.is_empty() {
        None
    } else if states.iter().any(|s| {
        [
            "FAILURE",
            "ERROR",
            "CANCELLED",
            "TIMED_OUT",
            "ACTION_REQUIRED",
            "STARTUP_FAILURE",
            "STALE",
        ]
        .contains(&s.as_str())
    }) {
        Some(CiState::Failed)
    } else if states.iter().any(|s| {
        [
            "PENDING",
            "EXPECTED",
            "QUEUED",
            "IN_PROGRESS",
            "WAITING",
            "REQUESTED",
        ]
        .contains(&s.as_str())
    }) {
        Some(CiState::Pending)
    } else if states
        .iter()
        .all(|s| ["SUCCESS", "NEUTRAL", "SKIPPED"].contains(&s.as_str()))
    {
        Some(CiState::Passed)
    } else {
        Some(CiState::Unknown)
    };

    let pr_fact = (|| {
        let state = match request["state"].as_str()? {
            "OPEN" if request["isDraft"] == true => PrState::Draft,
            "OPEN" => PrState::Open,
            "MERGED" => PrState::Merged,
            _ => return None,
        };
        (number > 0 && number <= MAX_SAFE_JSON_INTEGER).then_some(PrFact {
            number,
            url: request["url"].as_str()?.into(),
            state,
            ci: ci.unwrap_or(CiState::Unknown),
        })
    })();

    let merged = pr_fact.as_ref().is_some_and(|p| p.state == PrState::Merged);
    let ci_text = ci.filter(|_| !merged).map(|state| {
        match state {
            CiState::Passed => "CI passed",
            CiState::Failed => "CI failed",
            CiState::Pending => "CI pending",
            CiState::Unknown => "CI unknown",
        }
        .into()
    });

    WorkspaceFields {
        pr: (!merged).then(|| format!("PR #{number}")),
        ci: ci_text,
        pr_fact,
        ..Default::default()
    }
}
