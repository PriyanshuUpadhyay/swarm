// OpenUsage export and UTC timestamp formatting.
use super::*;

const OPENUSAGE_SCHEMA: &str = "openusage.limits.v1";

/// `swarm usage --format openusage`: cached meters in the OpenUsage `openusage.limits.v1` shape.
/// The rules are in `docs/usage-format.md`. The output has no token, home path, or email.
pub fn openusage(usage: &Usage, now: i64) -> Value {
    let mut accounts: Vec<(&str, &str)> = Vec::new();
    for meter in &usage.meters {
        let account = (
            meter.provider.as_str(),
            meter.account.as_deref().unwrap_or(&meter.label),
        );
        if !accounts.contains(&account) {
            accounts.push(account);
        }
    }
    let mut providers = serde_json::Map::new();
    let mut errors = Vec::new();
    for (provider, account) in accounts {
        let Some(info) = Provider::parse(provider).map(Provider::info) else {
            continue;
        };
        let rows: Vec<_> = usage
            .meters
            .iter()
            .filter(|meter| {
                meter.provider == provider
                    && meter.account.as_deref().unwrap_or(&meter.label) == account
            })
            .collect();
        // OpenUsage reads the provider family as the text before the first `@`.
        let (key, display) = if account == "default" {
            (provider.to_string(), info.label.to_string())
        } else {
            (
                format!("{provider}@{account}"),
                format!("{} ({account})", info.label),
            )
        };
        if let Some(reason) = rows
            .iter()
            .find(|meter| meter.state == "failed")
            .and_then(|meter| meter.reason.as_deref())
        {
            errors.push(json!({"providerId": key, "message": reason}));
        }
        let mut resources = serde_json::Map::new();
        for meter in &rows {
            let (Some(used), Some(name)) = (meter.used_pct, openusage_resource(meter)) else {
                continue;
            };
            let mut resource = json!({
                "kind": "consumption",
                "unit": "percent",
                "used": used,
                "limit": 100,
                "remaining": 100 - used,
                "utilization": used as f64 / 100.0,
            });
            if let Some(reset) = meter.reset_time_seconds {
                resource["resetsAt"] = iso_time(reset).into();
            }
            // windowSeconds is optional, so a length that overflows seconds is left out
            // and the export keeps the rest of the resource.
            if let Some(seconds) = meter
                .window_minutes
                .and_then(|minutes| minutes.checked_mul(60))
            {
                resource["windowSeconds"] = seconds.into();
            }
            resources.insert(name.into(), resource);
        }
        if resources.is_empty() {
            continue;
        }
        let mut entry = json!({"displayName": display});
        if let Some(fetched) = rows.iter().filter_map(|meter| meter.as_of_seconds).max() {
            entry["fetchedAt"] = iso_time(fetched).into();
            entry["expiresAt"] = iso_time(fetched + MAX_AGE_SECONDS).into();
        }
        entry["stale"] = rows.iter().any(|meter| meter.state == "stale").into();
        entry["resources"] = resources.into();
        providers.insert(key, entry);
    }
    json!({
        "schema": OPENUSAGE_SCHEMA,
        "generatedAt": iso_time(now),
        "providers": providers,
        "errors": errors,
    })
}

/// The OpenUsage resource key of a swarm window, or None for a window its table has no key for.
fn openusage_resource(meter: &UsageMeter) -> Option<&'static str> {
    match (meter.provider.as_str(), meter.window.as_deref()?) {
        ("claude", "5h") => Some("session"),
        ("claude", "7d") => Some("weekly"),
        ("claude", "fb") => Some("fable"),
        // Only the main Codex limit; another limit id, such as `review`, has no OpenUsage key.
        ("codex", "primary" | "secondary" | "codex:primary" | "codex:secondary") => {
            match meter.window_minutes? {
                300 => Some("session"),
                10080 => Some("weekly"),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `seconds` since 1970 as ISO 8601 in UTC with milliseconds, such as `2026-07-13T01:39:30.000Z`.
/// The day count to a date is the civil-from-days method of H. Hinnant, the inverse of the one
/// in `claude_reset_seconds`.
pub(super) fn iso_time(seconds: i64) -> String {
    let (days, second) = (seconds.div_euclid(86400), seconds.rem_euclid(86400));
    let shifted = days + 719468;
    let era = shifted.div_euclid(146097);
    let day_of_era = shifted.rem_euclid(146097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        second / 3600,
        second % 3600 / 60,
        second % 60
    )
}
