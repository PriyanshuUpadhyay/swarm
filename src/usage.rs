use crate::profiles::native::{AppServer, read_json, read_stdout};
use crate::profiles::{
    Account, AccountList, AuthState, ReasonCode, ResetCredits, Usage, UsageMeter,
};
use crate::providers::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const MAX_AGE_SECONDS: i64 = 300;
/// The version of the printed usage format (`docs/usage-format.md`).
pub const USAGE_SCHEMA: &str = "swarm.usage.v1";
const OPENUSAGE_SCHEMA: &str = "openusage.limits.v1";
/// The last second of year 9999. A reset time in text has the same limit.
const MAX_RESET_SECONDS: i64 = 253_402_300_799;

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    accounts: Vec<CachedAccount>,
}

#[derive(Serialize, Deserialize)]
struct CachedAccount {
    home: String,
    meters: Vec<UsageMeter>,
}

pub fn now_seconds() -> i64 {
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
    code: ReasonCode,
    text: Option<&str>,
) -> UsageMeter {
    let mut meter = UsageMeter {
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
        reason: None,
        reason_code: None,
        as_of_seconds: None,
        reset_credits: None,
    };
    meter.set_reason(code, text);
    meter
}

/// Read cached quota values; native identity calls share this deadline.
pub fn read(deadline: Instant) -> Result<Usage, String> {
    let now = now_seconds();
    let codex = crate::profiles::native::identities(Provider::Codex, deadline)?;
    let claude = crate::profiles::native::identities(Provider::Claude, deadline)?;
    let mut meters = fold_twins(&emails(&codex.accounts), for_accounts(&codex, now)?);
    meters.extend(fold_twins(
        &emails(&claude.accounts),
        for_accounts(&claude, now)?,
    ));
    meters.push(status_meter(
        "agy",
        None,
        "no_source",
        ReasonCode::NoSource,
        Some("No Swarm usage source"),
    ));
    Ok(Usage { meters })
}

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
fn iso_time(seconds: i64) -> String {
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

/// Each account's sign-in email, lowercased, for the accounts that have one.
fn emails(accounts: &[Account]) -> Vec<(String, String)> {
    accounts
        .iter()
        .filter_map(|account| {
            let email = account.email.as_deref()?.trim().to_lowercase();
            (!email.is_empty()).then(|| (account.name.clone(), email))
        })
        .collect()
}

/// Two accounts of one provider that sign in with one email are one account, so the meters show it
/// once: the account with a fresh reading wins, then one with any reading, then the one not named
/// `default`, then the first.
fn fold_twins(emails: &[(String, String)], meters: Vec<UsageMeter>) -> Vec<UsageMeter> {
    let reads = |name: &str, fresh: bool| {
        meters.iter().any(|meter| {
            meter.account.as_deref() == Some(name)
                && meter.used_pct.is_some()
                && (!fresh || meter.state == "fresh")
        })
    };
    let mut dropped = Vec::new();
    for (name, email) in emails {
        let twins: Vec<&str> = emails
            .iter()
            .filter(|(_, other)| other == email)
            .map(|(name, _)| name.as_str())
            .collect();
        let keep = twins
            .iter()
            .find(|twin| reads(twin, true))
            .or_else(|| twins.iter().find(|twin| reads(twin, false)))
            .or_else(|| twins.iter().find(|twin| **twin != "default"))
            .unwrap_or(&twins[0]);
        if name != keep {
            dropped.push(name.clone());
        }
    }
    meters
        .into_iter()
        .filter(|meter| {
            meter
                .account
                .as_ref()
                .is_none_or(|name| !dropped.contains(name))
        })
        .collect()
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
                ReasonCode::NoSource,
                if account.invalid_home() {
                    account.summary.as_deref()
                } else {
                    Some("API-key mode has no plan-quota source")
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
                    ReasonCode::NoCachedUsage,
                    None,
                )),
                None => meters.push(status_meter(
                    provider,
                    Some(account),
                    "missing",
                    ReasonCode::NoCachedUsage,
                    None,
                )),
            },
            Ok(None) => meters.push(status_meter(
                provider,
                Some(account),
                "missing",
                ReasonCode::NoCachedUsage,
                None,
            )),
            Err(_) => meters.push(status_meter(
                provider,
                Some(account),
                "failed",
                ReasonCode::ReadFailed,
                Some("Usage cache is unavailable"),
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
        || stored
            .reset_time_seconds
            .is_some_and(|time| !(0..=MAX_RESET_SECONDS).contains(&time));
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
        meter.reset_credits = None;
        // The app acts on these codes, so a cached read must keep them.
        let code = match (meter.state.as_str(), stored.reason_code) {
            ("missing", Some(code @ (ReasonCode::NoCredentials | ReasonCode::NoQuotaWindows))) => {
                code
            }
            ("missing", _) => ReasonCode::NoCachedUsage,
            ("no_source", _) => ReasonCode::NoSource,
            (_, Some(code @ (ReasonCode::ReloginRequired | ReasonCode::TokenExpired))) => code,
            _ => ReasonCode::ReadFailed,
        };
        meter.set_reason(code, None);
        return meter;
    }
    // A credit that expired since the read is gone, and the count drops with it.
    if let Some(credits) = &mut meter.reset_credits
        && let Some(list) = &mut credits.expiry_time_seconds
    {
        let before = list.len();
        list.retain(|at| at.is_none_or(|at| at > now));
        credits.available = (credits.available - (before - list.len()) as i64).max(0);
    }
    // Either half of the pair means a failed refresh kept this reading, so the pair stays whole.
    if stored.reason.is_some() || stored.reason_code.is_some() {
        meter.set_reason(ReasonCode::RefreshFailed, None);
    }
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

/// Refreshes `provider`'s usage for an automatic pick when its cache is missing or older than
/// MAX_AGE_SECONDS, so at most one refresh runs in each window, even when an account's read keeps
/// failing. None when the cache is new enough or the refresh fails; the pick then uses the cache.
pub fn refresh_if_old(
    provider: Provider,
    accounts: &AccountList,
    deadline: Instant,
) -> Option<Vec<UsageMeter>> {
    let age = std::fs::metadata(cache_path(provider.id()).ok()?)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok());
    if age.is_some_and(|age| age.as_secs() <= MAX_AGE_SECONDS as u64) {
        return None;
    }
    refresh_accounts(provider, accounts.accounts.clone(), deadline)
        .ok()
        .map(|usage| usage.meters)
}

fn refresh(provider: Provider, deadline: Instant) -> Result<Usage, String> {
    // The path check comes before the account read, which creates the swarm home.
    cache_path(provider.id())?;
    let accounts = crate::profiles::native::identities(provider, deadline)?.accounts;
    // Folded for display only; an automatic pick (refresh_if_old) needs every account's meters.
    let twins = emails(&accounts);
    let usage = refresh_accounts(provider, accounts, deadline)?;
    Ok(Usage {
        meters: fold_twins(&twins, usage.meters),
    })
}

fn refresh_accounts(
    provider: Provider,
    accounts: Vec<Account>,
    deadline: Instant,
) -> Result<Usage, String> {
    let path = cache_path(provider.id())?;
    let previous = cached_accounts(provider.id(), &accounts, now_seconds())?;
    let resolved_cache = resolved_parent(&path);
    if accounts.iter().any(|account| {
        resolved_cache.starts_with(resolved_parent(std::path::Path::new(&account.home)))
    }) {
        return Err("Swarm usage cache must be outside provider homes".into());
    }
    // Each read starts the CLI or a request, so the accounts are read at once under the deadline.
    let reads: Vec<_> = std::thread::scope(|scope| {
        let reads: Vec<_> = accounts
            .iter()
            .map(|account| {
                scope.spawn(move || match account.auth_state {
                    AuthState::SignedOut => None,
                    _ if provider == Provider::Codex => Some(fresh_codex(account, deadline)),
                    _ => Some(fresh_claude(account, deadline)),
                })
            })
            .collect();
        reads
            .into_iter()
            .map(|read| read.join().expect("a usage read panicked"))
            .collect()
    });
    let mut cache = Cache::default();
    let mut meters = Vec::new();
    for (account, read) in accounts.into_iter().zip(reads) {
        let account_meters = match read {
            None => vec![status_meter(
                provider.id(),
                Some(&account),
                "missing",
                ReasonCode::NoCredentials,
                None,
            )],
            Some(read) => match read {
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
                                meter.set_reason(ReasonCode::RefreshFailed, None);
                            }
                            meter
                        })
                        .collect();
                    if retained.is_empty() {
                        vec![status_meter(
                            provider.id(),
                            Some(&account),
                            "failed",
                            ReasonCode::ReadFailed,
                            Some(if provider == Provider::Codex {
                                "Codex usage read failed"
                            } else {
                                "Claude usage read failed"
                            }),
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
            ReasonCode::NoSource,
            account.summary.as_deref(),
        )]);
    }
    let storage = crate::providers::secure_storage_dir(&account.home, &account.name);
    let service = claude_keychain_service(storage.as_deref());
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
    // A missing or unreadable expiry counts as live, so the API answers for it.
    if credentials
        .pointer("/claudeAiOauth/expiresAt")
        .and_then(Value::as_i64)
        .is_some_and(|millis| millis < now_seconds().saturating_mul(1000))
    {
        return Ok(vec![status_meter(
            "claude",
            Some(account),
            "failed",
            ReasonCode::TokenExpired,
            None,
        )]);
    }
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
    let (response, status) = read_stdout(child, 1024 * 1024, deadline);
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
        b"401" | b"403" => {
            return Ok(vec![status_meter(
                "claude",
                Some(account),
                "failed",
                ReasonCode::ReloginRequired,
                None,
            )]);
        }
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
            ReasonCode::ReadFailed,
            Some("Claude usage window is invalid"),
        );
        meter.window = Some(window.into());
        meter.window_minutes = Some(minutes);
        if let (Some(percent), Some(reset_time)) = (percent, reset_time) {
            meter.used_pct = Some(percent.round() as i64);
            meter.reset_time_seconds = reset_time;
            meter.as_of_seconds = Some(now);
            meter.state = "fresh".into();
            meter.clear_reason();
        }
        meters.push(meter);
    }
    if meters.is_empty() {
        return Err("Claude usage response has no usage window".into());
    }
    Ok(meters)
}

fn claude_reset_seconds(value: &Value) -> Option<i64> {
    if let Some(seconds) = value.as_i64() {
        return (0..=MAX_RESET_SECONDS)
            .contains(&seconds)
            .then_some(seconds);
    }
    let text = value.as_str()?;
    if let Ok(seconds) = text.parse::<i64>() {
        return (0..=MAX_RESET_SECONDS)
            .contains(&seconds)
            .then_some(seconds);
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
        assert!(claude_windows(&account, &json!({}), 1000).is_err());
        for payload in [
            json!({"seven_day":{"utilization":40,"resets_at":"2026-10-12T01:02:03Z"}}),
            json!({"five_hour":null,"seven_day":{"utilization":40,"resets_at":"2026-10-12T01:02:03Z"}}),
        ] {
            let meters = claude_windows(&account, &payload, 1000).unwrap();
            assert_eq!(meters.len(), 1, "{payload}");
            assert_eq!(meters[0].window.as_deref(), Some("7d"));
            assert_eq!(meters[0].state, "fresh");
            assert_eq!(meters[0].used_pct, Some(40));
        }
        // A time in milliseconds is not a time in seconds. The limit is the end of year 9999,
        // as for a time in text.
        assert_eq!(
            claude_reset_seconds(&json!(253402300799_i64)),
            Some(253402300799)
        );
        for value in [
            json!(1791766923000_i64),
            json!("1791766923000"),
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
            fable_spent: false,
            renewable: false,
        }
    }

    #[test]
    fn accounts_with_one_email_show_once_and_the_one_with_a_reading_wins() {
        let meter = |account: &str, used: Option<i64>| UsageMeter {
            account: Some(account.into()),
            used_pct: used,
            ..status_meter("claude", None, "missing", ReasonCode::NoCachedUsage, None)
        };
        let emails = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(n, e)| (n.to_string(), e.to_string()))
                .collect()
        };
        let names = |meters: Vec<UsageMeter>| -> Vec<String> {
            meters
                .into_iter()
                .filter_map(|meter| meter.account)
                .collect()
        };
        let same = emails(&[
            ("default", "login-one"),
            ("me", "login-one"),
            ("team", "login-team"),
        ]);
        let read = vec![
            meter("default", Some(3)),
            meter("me", None),
            meter("team", Some(9)),
        ];
        assert_eq!(names(fold_twins(&same, read)), ["default", "team"]);
        let mut stale = meter("default", Some(40));
        stale.state = "stale".into();
        let mut fresh = meter("me", Some(5));
        fresh.state = "fresh".into();
        assert_eq!(names(fold_twins(&same, vec![stale, fresh])), ["me"]);
        let unread = vec![meter("default", None), meter("me", None)];
        assert_eq!(names(fold_twins(&same, unread)), ["me"]);
        let distinct = emails(&[("default", "login-a"), ("me", "login-b")]);
        let both = vec![meter("default", None), meter("me", None)];
        assert_eq!(names(fold_twins(&distinct, both)), ["default", "me"]);
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
        assert_eq!(unknown.reason_code, Some(ReasonCode::ReadFailed));
    }

    #[test]
    fn iso_time_gives_utc_with_milliseconds() {
        assert_eq!(iso_time(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_time(951_782_400), "2000-02-29T00:00:00.000Z");
        assert_eq!(iso_time(1_791_763_200), "2026-10-12T00:00:00.000Z");
        assert_eq!(iso_time(1_790_990_000), "2026-10-03T01:13:20.000Z");
    }

    #[test]
    fn openusage_export_keeps_only_known_windows_and_names_each_failed_account() {
        let meter =
            |provider: &str, account: Option<&str>, window: &str, minutes, used, state: &str| {
                let mut meter = UsageMeter {
                    provider: provider.into(),
                    account: account.map(str::to_string),
                    label: account.unwrap_or(provider).into(),
                    window: Some(window.into()),
                    window_minutes: Some(minutes),
                    used_pct: Some(used),
                    reset_time_seconds: Some(1_791_003_600),
                    state: state.into(),
                    source: None,
                    reason: None,
                    reason_code: None,
                    as_of_seconds: Some(1_791_000_000),
                    reset_credits: None,
                };
                if state == "stale" {
                    meter.set_reason(ReasonCode::RefreshFailed, None);
                    meter.as_of_seconds = Some(1_790_990_000);
                }
                meter
            };
        let mut weekly = meter("claude", Some("default"), "7d", 10080, 10, "fresh");
        weekly.reset_time_seconds = Some(1_791_604_800);
        let mut fable = meter("claude", Some("work"), "fb", 10080, 100, "stale");
        fable.reset_time_seconds = None;
        let mut codex = meter("codex", Some("default"), "codex:primary", 300, 30, "fresh");
        codex.as_of_seconds = Some(1_791_000_060);
        let account = |name: &str| Account {
            name: name.into(),
            ..work()
        };
        let usage = Usage {
            meters: vec![
                meter("claude", Some("default"), "5h", 300, 42, "fresh"),
                weekly,
                meter("claude", Some("work"), "5h", 300, 20, "stale"),
                fable,
                status_meter(
                    "claude",
                    Some(&account("broken")),
                    "failed",
                    ReasonCode::ReadFailed,
                    Some("Claude usage read failed"),
                ),
                status_meter(
                    "claude",
                    Some(&account("spare")),
                    "missing",
                    ReasonCode::NoCachedUsage,
                    None,
                ),
                codex,
                meter("codex", Some("default"), "review:primary", 60, 80, "fresh"),
                status_meter("agy", None, "no_source", ReasonCode::NoSource, None),
            ],
        };
        let resource = |used: i64, reset: Option<&str>| {
            let mut value = json!({
                "kind": "consumption", "unit": "percent", "used": used, "limit": 100,
                "remaining": 100 - used, "utilization": used as f64 / 100.0,
            });
            if let Some(reset) = reset {
                value["resetsAt"] = reset.into();
            }
            value
        };
        let with_window = |mut value: Value, seconds: i64| {
            value["windowSeconds"] = seconds.into();
            value
        };
        let expected = json!({
            "schema": "openusage.limits.v1",
            "generatedAt": "2026-10-03T04:01:40.000Z",
            "providers": {
                "claude": {
                    "displayName": "Claude",
                    "fetchedAt": "2026-10-03T04:00:00.000Z",
                    "expiresAt": "2026-10-03T04:05:00.000Z",
                    "stale": false,
                    "resources": {
                        "session": with_window(resource(42, Some("2026-10-03T05:00:00.000Z")), 18000),
                        "weekly": with_window(resource(10, Some("2026-10-10T04:00:00.000Z")), 604800),
                    },
                },
                "claude@work": {
                    "displayName": "Claude (work)",
                    "fetchedAt": "2026-10-03T01:13:20.000Z",
                    "expiresAt": "2026-10-03T01:18:20.000Z",
                    "stale": true,
                    "resources": {
                        "session": with_window(resource(20, Some("2026-10-03T05:00:00.000Z")), 18000),
                        "fable": with_window(resource(100, None), 604800),
                    },
                },
                "codex": {
                    "displayName": "Codex",
                    "fetchedAt": "2026-10-03T04:01:00.000Z",
                    "expiresAt": "2026-10-03T04:06:00.000Z",
                    "stale": false,
                    "resources": {
                        "session": with_window(resource(30, Some("2026-10-03T05:00:00.000Z")), 18000),
                    },
                },
            },
            "errors": [{"providerId": "claude@broken", "message": "Claude usage read failed"}],
        });
        assert_eq!(
            serde_json::to_string(&openusage(&usage, 1_791_000_100)).unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
    }

    #[test]
    fn openusage_leaves_out_a_window_length_that_does_not_fit_in_seconds() {
        let mut meter = status_meter(
            "claude",
            Some(&work()),
            "fresh",
            ReasonCode::ReadFailed,
            None,
        );
        meter.clear_reason();
        meter.window = Some("5h".into());
        meter.window_minutes = Some(i64::MAX);
        meter.used_pct = Some(10);
        meter.as_of_seconds = Some(1000);
        let value = openusage(
            &Usage {
                meters: vec![meter],
            },
            1000,
        );
        let session = &value["providers"]["claude@work"]["resources"]["session"];
        assert_eq!(session["used"], 10);
        assert!(session.get("windowSeconds").is_none(), "{session}");
    }

    #[test]
    fn a_cached_reading_with_half_a_reason_reads_back_as_a_kept_reading() {
        for (reason, code) in [
            (json!(null), json!("relogin_required")),
            (json!("x"), json!(null)),
        ] {
            let stored: UsageMeter = serde_json::from_value(json!({
                "provider": "claude", "account": "work", "label": "work", "window": "5h",
                "window_minutes": 300, "used_pct": 20, "reset_time_seconds": null, "state": "fresh",
                "source": "claude_api", "reason": reason, "reason_code": code, "as_of_seconds": 1000
            }))
            .unwrap();
            let cached = normalize_meter(&stored, 1000);
            assert_eq!(cached.used_pct, Some(20));
            assert_eq!(
                (cached.reason_code, cached.reason.as_deref()),
                (
                    Some(ReasonCode::RefreshFailed),
                    Some(ReasonCode::RefreshFailed.text())
                ),
                "{reason} {code}"
            );
        }
    }

    #[test]
    fn a_reset_time_past_year_9999_is_invalid_from_codex_and_from_the_cache() {
        // The Claude path rejects the same value (see claude_reset_seconds).
        let reply = json!({"rateLimits":{"primary":{"usedPercent":5,"windowDurationMins":300,"resetsAt":1791766923000_i64}}});
        assert_eq!(
            normalize_limits(&work(), &reply, 1000).unwrap_err(),
            "Invalid Codex usage window"
        );
        let bound =
            json!({"rateLimits":{"primary":{"usedPercent":5,"resetsAt":MAX_RESET_SECONDS}}});
        let mut meter = normalize_limits(&work(), &bound, 1000).unwrap().remove(0);
        assert_eq!(meter.reset_time_seconds, Some(MAX_RESET_SECONDS));
        assert_eq!(normalize_meter(&meter, 1000).state, "fresh");
        meter.reset_time_seconds = Some(MAX_RESET_SECONDS + 1);
        let cached = normalize_meter(&meter, 1000);
        assert_eq!(
            (cached.state.as_str(), cached.reset_time_seconds),
            ("failed", None)
        );
    }

    #[test]
    fn a_codex_reply_with_no_window_and_a_cached_read_keep_their_codes() {
        let empty = normalize_limits(&work(), &json!({"rateLimits":{}}), 1000).unwrap();
        assert_eq!(
            (empty[0].reason_code, empty[0].reason.as_deref()),
            (
                Some(ReasonCode::NoQuotaWindows),
                Some("No Codex quota windows")
            )
        );
        for (state, code) in [
            ("missing", ReasonCode::NoCredentials),
            ("missing", ReasonCode::NoQuotaWindows),
            ("failed", ReasonCode::ReloginRequired),
            ("failed", ReasonCode::TokenExpired),
        ] {
            let stored = status_meter("claude", Some(&work()), state, code, None);
            let cached = normalize_meter(&stored, 1000);
            assert_eq!(cached.reason_code, Some(code));
            assert_eq!(cached.reason.as_deref(), Some(code.text()));
        }
    }
    #[test]
    fn codex_reset_credits_ride_on_each_window_and_expired_ones_drop_on_read() {
        let reply = json!({
            "rateLimits": {"primary": {"usedPercent": 5}, "secondary": {"usedPercent": 9}},
            "rateLimitResetCredits": {"availableCount": 3, "credits": [
                {"status": "available", "expiresAt": null},
                {"status": "available", "expiresAt": 2000},
                {"status": "redeemed", "expiresAt": 1500},
                {"status": "available", "expiresAt": 1200},
            ]},
        });
        let meters = normalize_limits(&work(), &reply, 1000).unwrap();
        let credits = ResetCredits {
            available: 3,
            expiry_time_seconds: Some(vec![Some(1200), Some(2000), None]),
        };
        assert!(
            meters
                .iter()
                .all(|meter| meter.reset_credits == Some(credits.clone()))
        );
        // One credit expired after the read: the count drops with it.
        assert_eq!(
            normalize_meter(&meters[0], 1300).reset_credits,
            Some(ResetCredits {
                available: 2,
                expiry_time_seconds: Some(vec![Some(2000), None]),
            })
        );
        // A count with no list keeps the count; no field at all is no credits object.
        let count_only = json!({"rateLimits": {"primary": {"usedPercent": 5}},
                                "rateLimitResetCredits": {"availableCount": 1, "credits": null}});
        assert_eq!(
            normalize_limits(&work(), &count_only, 1000).unwrap()[0].reset_credits,
            Some(ResetCredits {
                available: 1,
                expiry_time_seconds: None
            })
        );
        let none = json!({"rateLimits": {"primary": {"usedPercent": 5}}});
        assert_eq!(
            normalize_limits(&work(), &none, 1000).unwrap()[0].reset_credits,
            None
        );
        // A failed cached meter has no reading, so it carries no credits.
        let mut broken = meters[0].clone();
        broken.used_pct = Some(101);
        assert_eq!(normalize_meter(&broken, 1000).reset_credits, None);
        // An old cache line has no field and still reads.
        let old: UsageMeter = serde_json::from_value(json!({"provider":"codex","account":"w","label":"w",
            "window":"primary","window_minutes":300,"used_pct":5,"reset_time_seconds":null,"state":"fresh",
            "source":null,"reason":null,"reason_code":null,"as_of_seconds":1000})).unwrap();
        assert_eq!(old.reset_credits, None);
    }
}
