// Usage cache reads, normalization, and refresh.
use super::*;

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

fn source_for(provider: &str, state: &str) -> Option<String> {
    match (provider, state) {
        (_, "no_source") => None,
        ("codex", _) => Some("codex_app_server".into()),
        ("claude", _) => Some("claude_api".into()),
        _ => None,
    }
}

pub(super) fn status_meter(
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
pub(super) fn fold_twins(emails: &[(String, String)], meters: Vec<UsageMeter>) -> Vec<UsageMeter> {
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
