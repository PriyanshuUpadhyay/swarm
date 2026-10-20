// Codex usage reads, windows, and reset credits.
use super::*;

pub(super) fn fresh_codex(account: &Account, deadline: Instant) -> Result<Vec<UsageMeter>, String> {
    // Invalid-name rows have no provider environment; do not read the default home for them.
    if account.invalid_home() {
        return Ok(vec![status_meter(
            "codex",
            Some(account),
            "no_source",
            ReasonCode::NoSource,
            account.summary.as_deref(),
        )]);
    }
    let mut server = AppServer::start(&account.env, deadline)?;
    let identity = server.request(2, "account/read", json!({"refreshToken":false}))?;
    if identity["account"]["type"] == "apiKey" {
        return Ok(vec![status_meter(
            "codex",
            Some(account),
            "no_source",
            ReasonCode::NoSource,
            Some("API-key mode has no plan-quota source"),
        )]);
    }
    if identity["account"]["type"] != "chatgpt" {
        return Err("No ChatGPT quota account".into());
    }
    let result = server.request(3, "account/rateLimits/read", json!({}))?;
    normalize_limits(account, &result, now_seconds())
}

pub(super) fn normalize_limits(
    account: &Account,
    result: &Value,
    now: i64,
) -> Result<Vec<UsageMeter>, String> {
    let mut buckets: BTreeMap<String, &Value> = BTreeMap::new();
    if let Some(values) = result["rateLimitsByLimitId"].as_object() {
        buckets.extend(values.iter().map(|(id, value)| (id.clone(), value)));
    }
    if buckets.is_empty() {
        let bucket = result
            .get("rateLimits")
            .filter(|bucket| bucket.is_object())
            .ok_or("No Codex limits")?;
        let id = bucket["limitId"].as_str().unwrap_or("");
        buckets.insert(id.into(), bucket);
    }
    let credits = reset_credits(&result["rateLimitResetCredits"]);
    let mut meters = Vec::new();
    for (id, bucket) in buckets {
        for window in ["primary", "secondary"] {
            let value = &bucket[window];
            if value.is_null() {
                continue;
            }
            let used_pct = value["usedPercent"]
                .as_i64()
                .filter(|percent| (0..=100).contains(percent))
                .ok_or("Invalid Codex usage percentage")?;
            let optional_integer =
                |key: &str, range: std::ops::RangeInclusive<i64>| -> Result<Option<i64>, String> {
                    match value.get(key).filter(|value| !value.is_null()) {
                        None => Ok(None),
                        Some(value) => value
                            .as_i64()
                            .filter(|number| range.contains(number))
                            .map(Some)
                            .ok_or_else(|| "Invalid Codex usage window".into()),
                    }
                };
            meters.push(UsageMeter {
                provider: "codex".into(),
                account: Some(account.name.clone()),
                label: account.name.clone(),
                // The fixed wire shape has no limit-id field, so retain it in the window name.
                window: Some(if id.is_empty() {
                    window.into()
                } else {
                    format!("{id}:{window}")
                }),
                window_minutes: optional_integer("windowDurationMins", 1..=i64::MAX)?,
                used_pct: Some(used_pct),
                // The Claude path has the same bound (`claude_reset_seconds`), so a time in
                // milliseconds is not read as a far-future second.
                reset_time_seconds: optional_integer("resetsAt", 0..=MAX_RESET_SECONDS)?,
                state: "fresh".into(),
                source: Some("codex_app_server".into()),
                reason: None,
                reason_code: None,
                as_of_seconds: Some(now),
                reset_credits: credits.clone(),
            });
        }
    }
    if meters.is_empty() {
        meters.push(status_meter(
            "codex",
            Some(account),
            "missing",
            ReasonCode::NoQuotaWindows,
            None,
        ));
    }
    Ok(meters)
}

/// `rateLimitResetCredits` of a Codex reply: the count, and the expiry of each listed credit that
/// is still available. None when the reply has no count.
fn reset_credits(value: &Value) -> Option<ResetCredits> {
    let available = value["availableCount"].as_i64()?.max(0);
    let expiry_time_seconds = value["credits"].as_array().map(|list| {
        let mut expiries: Vec<Option<i64>> = list
            .iter()
            .filter(|credit| credit["status"] == "available")
            .filter_map(|credit| match &credit["expiresAt"] {
                Value::Null => Some(None),
                at => at
                    .as_i64()
                    .filter(|at| (0..=MAX_RESET_SECONDS).contains(at))
                    .map(Some),
            })
            .collect();
        expiries.sort_by_key(|at| at.unwrap_or(i64::MAX));
        expiries
    });
    Some(ResetCredits {
        available,
        expiry_time_seconds,
    })
}
