use crate::profiles::native::{AppServer, read_json};
use crate::profiles::{Account, AccountList, AuthState, Usage, UsageMeter};
use crate::providers::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const MAX_AGE_SECONDS: i64 = 300;
const REFRESH_FAILED_REASON: &str = "Usage refresh failed; old reading";

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    accounts: Vec<CachedAccount>,
}

#[derive(Serialize, Deserialize)]
struct CachedAccount {
    home: String,
    meters: Vec<UsageMeter>,
}

pub(crate) fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn resolved_parent(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(path) = std::fs::canonicalize(path) {
        return path;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolved_parent(parent).join(name),
        _ => path.to_path_buf(),
    }
}

fn cache_path() -> Result<std::path::PathBuf, String> {
    let root = std::path::PathBuf::from(crate::paths::home().map_err(|error| error.to_string())?)
        .join(".swarm");
    let resolved_root = resolved_parent(&root);
    let home = std::path::PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    for provider in [Provider::Codex, Provider::Claude] {
        let variable = if provider == Provider::Codex {
            "CODEX_HOME"
        } else {
            "CLAUDE_CONFIG_DIR"
        };
        let active = std::env::var_os(variable)
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from);
        let discovery = crate::profiles::native::discover(provider, &home, active);
        if discovery
            .homes
            .iter()
            .chain(&discovery.skipped)
            .any(|(_, path)| resolved_root.starts_with(resolved_parent(path)))
        {
            return Err("Swarm usage cache must be outside provider homes".into());
        }
    }
    let metadata = crate::accounts::load()?;
    if metadata
        .accounts
        .iter()
        .any(|entry| resolved_root.starts_with(resolved_parent(&entry.home)))
    {
        return Err("Swarm usage cache must be outside provider homes".into());
    }
    crate::paths::root_dir()
        .map(|root| root.join("codex-usage.json"))
        .map_err(|error| error.to_string())
}

fn yelo_command() -> String {
    std::env::var("SWARM_YELO_CMD").unwrap_or_else(|_| "yelo".into())
}

pub(crate) fn claude_snapshot(deadline: Instant) -> Result<Value, String> {
    let executable = yelo_command();
    read_json(
        &executable,
        &["usage", "show", "--json"],
        &BTreeMap::new(),
        deadline,
    )
    .map_err(String::from)
}

pub(crate) fn window_minutes(window: &str) -> Option<i64> {
    let (number, multiplier) =
        [("m", 1), ("h", 60), ("d", 1440)]
            .into_iter()
            .find_map(|(unit, multiplier)| {
                window.strip_suffix(unit).map(|number| (number, multiplier))
            })?;
    let number = number.parse::<i64>().ok().filter(|number| *number > 0)?;
    number.checked_mul(multiplier)
}

fn source_for(provider: &str, state: &str) -> Option<String> {
    match (provider, state) {
        (_, "no_source") => None,
        ("codex", _) => Some("codex_app_server".into()),
        ("claude", _) => Some("yelo".into()),
        _ => None,
    }
}

fn status_meter(
    provider: &str,
    account: Option<&Account>,
    state: &str,
    reason: &str,
) -> UsageMeter {
    UsageMeter {
        provider: provider.into(),
        account: account.map(|account| account.name.clone()),
        label: account
            .map(|account| account.name.clone())
            .unwrap_or_else(|| provider.into()),
        window: None,
        window_minutes: None,
        used_pct: None,
        reset_time_seconds: None,
        state: state.into(),
        source: source_for(provider, state),
        reason: Some(reason.into()),
        as_of_seconds: None,
    }
}

/// Read cached quota values; native identity and Claude snapshot calls share this deadline.
pub fn read(deadline: Instant) -> Result<Usage, String> {
    let now = now_seconds();
    let codex = crate::profiles::native::identities(Provider::Codex, deadline)?;
    let claude = crate::profiles::native::identities(Provider::Claude, deadline)?;
    let mut meters = for_accounts(&codex, deadline, now)?;
    meters.extend(for_accounts(&claude, deadline, now)?);
    meters.push(status_meter(
        "agy",
        None,
        "no_source",
        "No Swarm usage source",
    ));
    Ok(Usage { meters })
}

pub(crate) fn for_accounts(
    list: &AccountList,
    deadline: Instant,
    now: i64,
) -> Result<Vec<UsageMeter>, String> {
    if list.provider == "codex" {
        return cached_codex(&list.accounts, now);
    }
    let claude = list;
    let mut meters: Vec<_> = claude
        .accounts
        .iter()
        .filter(|account| account.invalid_home())
        .map(|account| {
            status_meter(
                "claude",
                Some(account),
                "no_source",
                account.summary.as_deref().unwrap_or("No usage source"),
            )
        })
        .collect();
    match claude_snapshot(deadline).and_then(|value| {
        crate::profiles::translate_usage(&value.to_string(), std::slice::from_ref(claude), now)
    }) {
        Ok((usage, skipped)) => {
            for account in claude
                .accounts
                .iter()
                .filter(|account| !account.invalid_home())
            {
                if !usage
                    .meters
                    .iter()
                    .any(|meter| meter.account.as_deref() == Some(account.name.as_str()))
                {
                    meters.push(status_meter(
                        "claude",
                        Some(account),
                        if skipped.is_empty() {
                            "missing"
                        } else {
                            "failed"
                        },
                        if skipped.is_empty() {
                            "No cached Claude usage"
                        } else {
                            "Claude usage row is invalid"
                        },
                    ));
                }
            }
            meters.extend(usage.meters);
        }
        Err(_) => meters.extend(
            claude
                .accounts
                .iter()
                .filter(|account| !account.invalid_home())
                .map(|account| {
                    status_meter(
                        "claude",
                        Some(account),
                        "failed",
                        "Claude usage source is unavailable",
                    )
                }),
        ),
    }
    Ok(meters)
}

fn cached_codex(accounts: &[Account], now: i64) -> Result<Vec<UsageMeter>, String> {
    let stored = match cache_path() {
        Err(_) => Err("refused cache"),
        Ok(path) => match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Cache>(&bytes)
                .map(Some)
                .map_err(|_| "invalid cache"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("unavailable cache"),
        },
    };
    let mut meters = Vec::new();
    for account in accounts {
        if account.usage_state == "no_source" {
            meters.push(status_meter(
                "codex",
                Some(account),
                "no_source",
                if account.invalid_home() {
                    account.summary.as_deref().unwrap_or("No usage source")
                } else {
                    "API-key mode has no plan-quota source"
                },
            ));
            continue;
        }
        match &stored {
            Ok(Some(cache)) => match cache
                .accounts
                .iter()
                .find(|entry| entry.home == account.home)
            {
                Some(entry) if !entry.meters.is_empty() => {
                    meters.extend(entry.meters.iter().map(|meter| {
                        let mut meter = meter.clone();
                        meter.provider = "codex".into();
                        meter.account = Some(account.name.clone());
                        meter.label = account.name.clone();
                        normalize_meter(&meter, now)
                    }));
                }
                Some(_) => meters.push(status_meter(
                    "codex",
                    Some(account),
                    "missing",
                    "No cached Codex usage",
                )),
                None => meters.push(status_meter(
                    "codex",
                    Some(account),
                    "missing",
                    "No cached Codex usage",
                )),
            },
            Ok(None) => meters.push(status_meter(
                "codex",
                Some(account),
                "missing",
                "No cached Codex usage",
            )),
            Err(_) => meters.push(status_meter(
                "codex",
                Some(account),
                "failed",
                "Codex cache is unavailable",
            )),
        }
    }
    Ok(meters)
}

pub(crate) fn normalize_meter(stored: &UsageMeter, now: i64) -> UsageMeter {
    let mut meter = stored.clone();
    meter.source = source_for(&meter.provider, &meter.state);
    let invalid_value = stored
        .used_pct
        .is_some_and(|percent| !(0..=100).contains(&percent))
        || stored.window_minutes.is_some_and(|minutes| minutes <= 0)
        || stored.reset_time_seconds.is_some_and(|time| time < 0);
    if !matches!(stored.state.as_str(), "fresh" | "stale")
        || stored.used_pct.is_none()
        || invalid_value
    {
        meter.state = match stored.state.as_str() {
            "missing" | "no_source" if !invalid_value => stored.state.clone(),
            _ => "failed".into(),
        };
        meter.used_pct = None;
        meter.window_minutes = meter.window_minutes.filter(|minutes| *minutes > 0);
        meter.reset_time_seconds = None;
        meter.as_of_seconds = None;
        meter.reason = Some(
            match meter.state.as_str() {
                "missing" => "No cached usage",
                "no_source" => "No usage source",
                _ => "Usage read failed",
            }
            .into(),
        );
        return meter;
    }
    meter.reason = stored.reason.as_ref().map(|_| REFRESH_FAILED_REASON.into());
    if meter
        .as_of_seconds
        .is_none_or(|time| time < 0 || time > now || now - time > MAX_AGE_SECONDS)
    {
        meter.state = "stale".into();
        meter.as_of_seconds = meter
            .as_of_seconds
            .filter(|time| *time >= 0 && *time <= now);
    }
    meter
}

/// Refresh Codex only and atomically replace its nonsecret cache outside provider homes.
/// Every identity and quota request uses the supplied total deadline.
pub fn refresh_codex(deadline: Instant) -> Result<Usage, String> {
    let path = cache_path()?;
    let accounts = crate::profiles::native::identities(Provider::Codex, deadline)?.accounts;
    let previous = cached_codex(&accounts, now_seconds())?;
    let resolved_cache = resolved_parent(&path);
    if accounts.iter().any(|account| {
        resolved_cache.starts_with(resolved_parent(std::path::Path::new(&account.home)))
    }) {
        return Err("Swarm usage cache must be outside provider homes".into());
    }
    let mut cache = Cache::default();
    let mut meters = Vec::new();
    for account in accounts {
        let account_meters = match account.auth_state {
            AuthState::SignedOut => vec![status_meter(
                "codex",
                Some(&account),
                "missing",
                "Account is signed out",
            )],
            _ => match fresh_codex(&account, deadline) {
                Ok(meters) => meters,
                Err(_) => {
                    let retained: Vec<_> = previous
                        .iter()
                        .filter(|meter| {
                            meter.account.as_deref() == Some(&account.name)
                                && matches!(meter.state.as_str(), "fresh" | "stale" | "failed")
                        })
                        .map(|meter| {
                            let mut meter = meter.clone();
                            if meter.state != "failed" {
                                meter.state = "stale".into();
                                meter.reason = Some(REFRESH_FAILED_REASON.into());
                            }
                            meter
                        })
                        .collect();
                    if retained.is_empty() {
                        vec![status_meter(
                            "codex",
                            Some(&account),
                            "failed",
                            "Codex usage read failed",
                        )]
                    } else {
                        retained
                    }
                }
            },
        };
        cache.accounts.push(CachedAccount {
            home: account.home,
            meters: account_meters.clone(),
        });
        meters.extend(account_meters);
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "Cannot write Codex usage cache")?;
    let written = serde_json::to_writer(&mut file, &cache)
        .map_err(|_| "Cannot encode Codex usage cache")
        .and_then(|_| file.flush().map_err(|_| "Cannot flush Codex usage cache"))
        .and_then(|_| file.sync_all().map_err(|_| "Cannot sync Codex usage cache"))
        .and_then(|_| {
            std::fs::rename(&temporary, &path).map_err(|_| "Cannot replace Codex usage cache")
        });
    if written.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    written?;
    Ok(Usage { meters })
}

fn fresh_codex(account: &Account, deadline: Instant) -> Result<Vec<UsageMeter>, String> {
    // Invalid-name rows have no provider environment; do not read the default home for them.
    if account.invalid_home() {
        return Ok(vec![status_meter(
            "codex",
            Some(account),
            "no_source",
            account.summary.as_deref().unwrap_or("No usage source"),
        )]);
    }
    let mut server = AppServer::start(&account.env, deadline)?;
    let identity = server.request(2, "account/read", json!({"refreshToken":false}))?;
    if identity["account"]["type"] == "apiKey" {
        return Ok(vec![status_meter(
            "codex",
            Some(account),
            "no_source",
            "API-key mode has no plan-quota source",
        )]);
    }
    if identity["account"]["type"] != "chatgpt" {
        return Err("No ChatGPT quota account".into());
    }
    let result = server.request(3, "account/rateLimits/read", json!({}))?;
    normalize_limits(account, &result, now_seconds())
}

fn normalize_limits(
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
            let optional_integer = |key: &str, minimum: i64| -> Result<Option<i64>, String> {
                match value.get(key).filter(|value| !value.is_null()) {
                    None => Ok(None),
                    Some(value) => value
                        .as_i64()
                        .filter(|number| *number >= minimum)
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
                window_minutes: optional_integer("windowDurationMins", 1)?,
                used_pct: Some(used_pct),
                reset_time_seconds: optional_integer("resetsAt", 0)?,
                state: "fresh".into(),
                source: Some("codex_app_server".into()),
                reason: None,
                as_of_seconds: Some(now),
            });
        }
    }
    if meters.is_empty() {
        meters.push(status_meter(
            "codex",
            Some(account),
            "missing",
            "No Codex quota windows",
        ));
    }
    Ok(meters)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> Account {
        Account {
            name: "work".into(),
            email: None,
            home: "/tmp/demo/.codex-work".into(),
            env: BTreeMap::new(),
            auth_state: AuthState::SignedIn,
            remaining_pct: None,
            usage_state: "missing".into(),
            usage_source: Some("codex_app_server".into()),
            summary: None,
        }
    }

    #[test]
    fn cache_age_boundary_future_time_and_bad_states_stay_explicit() {
        let account = work();
        let mut meter = normalize_limits(
            &account,
            &json!({"rateLimits":{"primary":{"usedPercent":0}}}),
            1000,
        )
        .unwrap()
        .remove(0);
        assert_eq!(normalize_meter(&meter, 1300).state, "fresh");
        assert_eq!(normalize_meter(&meter, 1301).state, "stale");
        assert_eq!(normalize_meter(&meter, 999).state, "stale");
        meter.as_of_seconds = Some(i64::MIN);
        assert_eq!(normalize_meter(&meter, 1000).state, "stale");
        meter.state = "new-provider-state".into();
        meter.reason = Some("private server diagnostics".into());
        let unknown = normalize_meter(&meter, 1000);
        assert_eq!(unknown.state, "failed");
        assert_eq!(unknown.used_pct, None);
        assert!(!unknown.reason.unwrap().contains("private"));
    }

    #[test]
    fn quota_duration_rejects_bad_units_unicode_and_overflow() {
        assert_eq!(window_minutes("5h"), Some(300));
        assert_eq!(window_minutes("7d"), Some(10080));
        for window in ["", "é", "1é", "0h", "-1h", "1w", "9223372036854775807d"] {
            assert_eq!(window_minutes(window), None);
        }
    }
}
