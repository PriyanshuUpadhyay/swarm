use crate::profiles::native::{AppServer, read_json};
use crate::profiles::{Account, AccountList, AuthState, Usage, UsageMeter};
use crate::providers::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
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

fn cache_path(provider: &str) -> Result<std::path::PathBuf, String> {
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
        .map(|root| root.join(format!("{provider}-usage.json")))
        .map_err(|error| error.to_string())
}

fn claude_keychain_service(secure_storage: Option<&str>) -> String {
    match secure_storage {
        None => "Claude Code-credentials".into(),
        Some(path) => {
            let hash = Sha256::digest(path.as_bytes());
            format!(
                "Claude Code-credentials-{:02x}{:02x}{:02x}{:02x}",
                hash[0], hash[1], hash[2], hash[3]
            )
        }
    }
}

fn source_for(provider: &str, state: &str) -> Option<String> {
    match (provider, state) {
        (_, "no_source") => None,
        ("codex", _) => Some("codex_app_server".into()),
        ("claude", _) => Some("claude_api".into()),
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

/// Read cached quota values; native identity calls share this deadline.
pub fn read(deadline: Instant) -> Result<Usage, String> {
    let now = now_seconds();
    let codex = crate::profiles::native::identities(Provider::Codex, deadline)?;
    let claude = crate::profiles::native::identities(Provider::Claude, deadline)?;
    let mut meters = for_accounts(&codex, now)?;
    meters.extend(for_accounts(&claude, now)?);
    meters.push(status_meter(
        "agy",
        None,
        "no_source",
        "No Swarm usage source",
    ));
    Ok(Usage { meters })
}

pub(crate) fn for_accounts(list: &AccountList, now: i64) -> Result<Vec<UsageMeter>, String> {
    cached_accounts(&list.provider, &list.accounts, now)
}

fn cached_accounts(
    provider: &str,
    accounts: &[Account],
    now: i64,
) -> Result<Vec<UsageMeter>, String> {
    let stored = match cache_path(provider) {
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
                provider,
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
                        meter.provider = provider.into();
                        meter.account = Some(account.name.clone());
                        meter.label = account.name.clone();
                        normalize_meter(&meter, now)
                    }));
                }
                Some(_) => meters.push(status_meter(
                    provider,
                    Some(account),
                    "missing",
                    "No cached usage",
                )),
                None => meters.push(status_meter(
                    provider,
                    Some(account),
                    "missing",
                    "No cached usage",
                )),
            },
            Ok(None) => meters.push(status_meter(
                provider,
                Some(account),
                "missing",
                "No cached usage",
            )),
            Err(_) => meters.push(status_meter(
                provider,
                Some(account),
                "failed",
                "Usage cache is unavailable",
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
                _ if stored.reason.as_deref() == Some("Sign in again") => "Sign in again",
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
    refresh(Provider::Codex, deadline)
}

pub fn refresh_claude(deadline: Instant) -> Result<Usage, String> {
    refresh(Provider::Claude, deadline)
}

fn refresh(provider: Provider, deadline: Instant) -> Result<Usage, String> {
    let path = cache_path(provider.id())?;
    let accounts = crate::profiles::native::identities(provider, deadline)?.accounts;
    let previous = cached_accounts(provider.id(), &accounts, now_seconds())?;
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
                provider.id(),
                Some(&account),
                "missing",
                "Account is signed out",
            )],
            _ => match if provider == Provider::Codex {
                fresh_codex(&account, deadline)
            } else {
                fresh_claude(&account, deadline)
            } {
                Ok(meters) => meters,
                Err(reason) if reason == "Sign in again" => vec![status_meter(
                    provider.id(),
                    Some(&account),
                    "failed",
                    "Sign in again",
                )],
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
                            provider.id(),
                            Some(&account),
                            "failed",
                            if provider == Provider::Codex {
                                "Codex usage read failed"
                            } else {
                                "Claude usage read failed"
                            },
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
        .map_err(|_| "Cannot write usage cache")?;
    let written = serde_json::to_writer(&mut file, &cache)
        .map_err(|_| "Cannot encode usage cache")
        .and_then(|_| file.flush().map_err(|_| "Cannot flush usage cache"))
        .and_then(|_| file.sync_all().map_err(|_| "Cannot sync usage cache"))
        .and_then(|_| std::fs::rename(&temporary, &path).map_err(|_| "Cannot replace usage cache"));
    if written.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    written?;
    Ok(Usage { meters })
}

fn fresh_claude(account: &Account, deadline: Instant) -> Result<Vec<UsageMeter>, String> {
    if account.invalid_home() {
        return Ok(vec![status_meter(
            "claude",
            Some(account),
            "no_source",
            account.summary.as_deref().unwrap_or("No usage source"),
        )]);
    }
    let service = claude_keychain_service(
        account
            .env
            .get("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .map(String::as_str),
    );
    let security = std::env::var("SWARM_SECURITY_CMD").unwrap_or_else(|_| "security".into());
    let user = std::env::var("USER").unwrap_or_default();
    let credentials = read_json(
        &security,
        &["find-generic-password", "-s", &service, "-a", &user, "-w"],
        &BTreeMap::new(),
        deadline,
    )
    .map_err(|_| "Claude credentials are unavailable")?;
    let token = credentials
        .pointer("/claudeAiOauth/accessToken")
        .and_then(Value::as_str)
        .filter(|token| {
            !token.is_empty()
                && token.len() <= 8192
                && token.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .ok_or("Claude credentials are unavailable")?;
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or("Claude usage read timed out")?;
    let curl = std::env::var("SWARM_CURL_CMD").unwrap_or_else(|_| "curl".into());
    let url = std::env::var("SWARM_CLAUDE_USAGE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com/api/oauth/usage".into());
    let mut child = Command::new(curl)
        // Disable curlrc so it cannot enable redirects, tracing, or a different destination.
        .args([
            "-q",
            "--silent",
            "--max-time",
            &remaining.as_secs_f64().to_string(),
            "--write-out",
            "\n%{http_code}",
            "-H",
            "@-",
            "--url",
            &url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Claude usage request is unavailable")?;
    let sent = child.stdin.take().is_some_and(|mut stdin| {
        writeln!(
            stdin,
            "Authorization: Bearer {token}\nanthropic-beta: oauth-2025-04-20"
        )
        .is_ok()
    });
    if !sent {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Claude usage request failed".into());
    }
    let stdout = child
        .stdout
        .take()
        .ok_or("Claude usage response is unavailable")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(1024 * 1024)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    let response = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let mut status = child.try_wait().ok().flatten();
    while response.is_ok() && status.is_none() && Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(2));
        status = child.try_wait().ok().flatten();
    }
    let _ = child.kill();
    let _ = child.wait();
    if !status.is_some_and(|status| status.success()) {
        return Err("Claude usage request failed".into());
    }
    let bytes = response
        .map_err(|_| "Claude usage read timed out")?
        .map_err(|_| "Claude usage read failed")?;
    let split = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or("Claude usage response is invalid")?;
    match &bytes[split + 1..] {
        b"401" | b"403" => return Err("Sign in again".into()),
        b"200" => (),
        _ => return Err("Claude usage request failed".into()),
    }
    let payload =
        serde_json::from_slice(&bytes[..split]).map_err(|_| "Claude usage response is invalid")?;
    claude_windows(account, &payload, now_seconds())
}

fn claude_windows(account: &Account, payload: &Value, now: i64) -> Result<Vec<UsageMeter>, String> {
    let probe = |value: &Value, names: &[&str]| {
        names
            .iter()
            .find_map(|name| {
                value
                    .get(*name)
                    .filter(|value| !value.is_null() && **value != false)
            })
            .cloned()
            .unwrap_or(Value::Null)
    };
    let mut fable = probe(
        payload,
        &["fable", "seven_day_fable", "fable_seven_day", "fable_week"],
    );
    if fable.is_null()
        && let Some(limits) = payload["limits"].as_array()
    {
        fable = limits
            .iter()
            .find(|entry| {
                entry["kind"] == "weekly_scoped"
                    && entry
                        .pointer("/scope/model/display_name")
                        .and_then(Value::as_str)
                        == Some("Fable")
            })
            .cloned()
            .unwrap_or(Value::Null);
    }
    let mut meters = Vec::new();
    for (window, minutes, value) in [
        (
            "5h",
            300,
            probe(payload, &["five_hour", "fiveHour", "five_hour_window"]),
        ),
        (
            "7d",
            10080,
            probe(payload, &["seven_day", "sevenDay", "seven_day_window"]),
        ),
        ("fb", 10080, fable),
    ] {
        if value.is_null() {
            continue;
        }
        let percent = probe(
            &value,
            &["used_percentage", "utilization", "used_percent", "percent"],
        );
        let percent = percent
            .as_f64()
            .or_else(|| percent.as_str()?.parse().ok())
            .filter(|percent| percent.is_finite() && (0.0..=100.0).contains(percent));
        let reset = probe(&value, &["resets_at", "reset_at", "resets"]);
        let reset_time = if reset.is_null() {
            Some(None)
        } else {
            claude_reset_seconds(&reset).map(Some)
        };
        let mut meter = status_meter(
            "claude",
            Some(account),
            "failed",
            "Claude usage window is invalid",
        );
        meter.window = Some(window.into());
        meter.window_minutes = Some(minutes);
        if let (Some(percent), Some(reset_time)) = (percent, reset_time) {
            meter.used_pct = Some(percent.round() as i64);
            meter.reset_time_seconds = reset_time;
            meter.as_of_seconds = Some(now);
            meter.state = "fresh".into();
            meter.reason = None;
        }
        meters.push(meter);
    }
    if !meters
        .iter()
        .any(|meter| meter.window.as_deref() == Some("5h"))
    {
        return Err("Claude usage response has no five-hour window".into());
    }
    Ok(meters)
}

fn claude_reset_seconds(value: &Value) -> Option<i64> {
    if let Some(seconds) = value.as_i64() {
        return (seconds >= 0).then_some(seconds);
    }
    let text = value.as_str()?;
    if let Ok(seconds) = text.parse::<i64>() {
        return (seconds >= 0).then_some(seconds);
    }
    let text = text
        .strip_suffix('Z')
        .or_else(|| text.strip_suffix("+00:00"))?;
    let (text, fraction) = text
        .split_once('.')
        .map_or((text, None), |(text, fraction)| (text, Some(fraction)));
    if !text.is_ascii()
        || text.len() != 19
        || fraction.is_some_and(|value| {
            value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return None;
    }
    if [4, 7, 10, 13, 16].map(|index| text.as_bytes()[index]) != *b"--T::" {
        return None;
    }
    let number = |start, end| text[start..end].parse::<i64>().ok();
    let (year, month, day, hour, minute, second) = (
        number(0, 4)?,
        number(5, 7)?,
        number(8, 10)?,
        number(11, 13)?,
        number(14, 16)?,
        number(17, 19)?,
    );
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if !(1970..=9999).contains(&year)
        || !(1..=days).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..60).contains(&second)
    {
        return None;
    }
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some((era * 146097 + day_of_era - 719468) * 86400 + hour * 3600 + minute * 60 + second)
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

    #[test]
    fn keychain_service_matches_the_adopted_profile_vector() {
        assert_eq!(claude_keychain_service(None), "Claude Code-credentials");
        assert_eq!(
            claude_keychain_service(Some("/Users/example/.claude-work")),
            "Claude Code-credentials-dd1118a7"
        );
    }

    #[test]
    fn claude_windows_keep_failed_values_and_accept_the_api_aliases() {
        let mut account = work();
        account.name = "personal".into();
        let meters = claude_windows(
            &account,
            &json!({
                "fiveHour":{"used_percent":"12.6","reset_at":"2026-10-12T01:02:03.123Z"},
                "sevenDay":{"percent":101},
                "seven_day_fable":{"utilization":90}
            }),
            1000,
        )
        .unwrap();
        assert_eq!(meters[0].window.as_deref(), Some("5h"));
        assert_eq!(meters[0].used_pct, Some(13));
        assert_eq!(meters[0].reset_time_seconds, Some(1791766923));
        assert_eq!(meters[0].as_of_seconds, Some(1000));
        assert_eq!(meters[1].state, "failed");
        assert_eq!(meters[1].used_pct, None);
        assert_eq!(meters[2].window.as_deref(), Some("fb"));
        assert_eq!(meters[2].window_minutes, Some(10080));
        for payload in [json!({}), json!({"seven_day":{"utilization":10}})] {
            assert!(claude_windows(&account, &payload, 1000).is_err());
        }
        for value in [
            json!(-1),
            json!("2026-02-29T01:02:03Z"),
            json!("2026-10-12T24:02:03Z"),
            json!("2026-10-12T01:02:03.éZ"),
            json!("２０２６-10-12T01:02:03Z"),
        ] {
            assert_eq!(claude_reset_seconds(&value), None);
        }
        assert_eq!(
            claude_reset_seconds(&json!("2024-02-29T00:00:00Z")),
            Some(1709164800)
        );
    }

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
            invalid_home: false,
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
}
