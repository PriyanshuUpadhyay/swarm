use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountList {
    pub provider: String,
    pub source: Option<String>,
    pub state: String,
    pub revision: String,
    pub modified: bool,
    pub accounts: Vec<Account>,
    pub auto: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Account {
    pub name: String,
    pub email: Option<String>,
    pub home: String,
    pub env: BTreeMap<String, String>,
    pub auth_state: AuthState,
    pub usage_state: String,
    pub usage_source: Option<String>,
    pub remaining_pct: Option<i64>,
    pub summary: Option<String>,
}

pub mod native;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    SignedIn,
    SignedOut,
    Unavailable,
}

pub fn empty_accounts(provider: &str) -> AccountList {
    AccountList {
        provider: provider.to_string(),
        source: None,
        state: "no_source".into(),
        revision: crate::config::revision(b"version = 1\n"),
        modified: false,
        accounts: Vec::new(),
        auto: None,
    }
}

pub fn pick_auto(accounts: &[Account], current_home: Option<&str>) -> Option<String> {
    let signed_in = || {
        accounts
            .iter()
            .filter(|account| account.auth_state == AuthState::SignedIn)
    };
    signed_in()
        .filter(|account| account.usage_state == "fresh" && account.remaining_pct.is_some())
        .min_by(|left, right| {
            right
                .remaining_pct
                .cmp(&left.remaining_pct)
                .then(left.name.cmp(&right.name))
        })
        .or_else(|| signed_in().find(|account| Some(account.home.as_str()) == current_home))
        .map(|account| account.name.clone())
}

pub fn apply_cached_usage(list: &mut AccountList, usage: &serde_json::Value, now: i64) {
    for account in &mut list.accounts {
        if account.usage_state == "no_source" {
            continue;
        }
        let rows: Vec<&serde_json::Value> = if list.provider == "codex" {
            usage["accounts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| entry["home"].as_str() == Some(account.home.as_str()))
                .flat_map(|entry| entry["meters"].as_array().into_iter().flatten())
                .collect()
        } else {
            usage
                .as_array()
                .into_iter()
                .flatten()
                .filter(|row| {
                    row["provider"] == "claude"
                        && row["label"]
                            .as_str()
                            .and_then(|label| label.split_once('·').map(|(_, tail)| tail))
                            .is_some_and(|tail| {
                                tail == account.name || account.email.as_deref() == Some(tail)
                            })
                })
                .collect()
        };
        let mut remaining = Vec::new();
        let mut fresh = true;
        for row in rows {
            let state = row["state"].as_str().unwrap_or("missing");
            if !matches!(state, "fresh" | "ok" | "stale") {
                account.usage_state = match state {
                    "failed" | "no_source" => state,
                    _ => "missing",
                }
                .into();
                continue;
            }
            let percent = row
                .get("used_pct")
                .or_else(|| row.get("pct"))
                .and_then(serde_json::Value::as_i64);
            let Some(percent) = percent.filter(|percent| (0..=100).contains(percent)) else {
                continue;
            };
            let time = row
                .get("as_of_seconds")
                .or_else(|| row.get("asOf"))
                .and_then(serde_json::Value::as_i64);
            fresh &= matches!(state, "fresh" | "ok")
                && time.is_some_and(|time| {
                    time >= 0 && time <= now && now - time <= crate::usage::MAX_AGE_SECONDS
                });
            remaining.push(100 - percent);
        }
        if let Some(remaining) = remaining.into_iter().min() {
            account.remaining_pct = Some(remaining);
            account.usage_state = if fresh { "fresh" } else { "stale" }.into();
            account.summary = Some(format!("{remaining}% left"));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub meters: Vec<UsageMeter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageMeter {
    pub provider: String,
    pub account: Option<String>,
    pub label: String,
    pub window: Option<String>,
    pub window_minutes: Option<i64>,
    pub used_pct: Option<i64>,
    pub reset_time_seconds: Option<i64>,
    pub state: String,
    pub source: Option<String>,
    pub reason: Option<String>,
    pub as_of_seconds: Option<i64>,
}

#[derive(Deserialize)]
struct YeloUsageMeter {
    label: String,
    window: Option<String>,
    pct: Option<i64>,
    state: String,
    #[serde(rename = "asOf")]
    as_of: Option<i64>,
}

pub fn translate_usage(
    json: &str,
    account_lists: &[AccountList],
    now: i64,
) -> Result<(Usage, Vec<String>), String> {
    let input: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|_| "Claude usage JSON is invalid")?;
    let mut skipped = Vec::new();
    let mut meters = Vec::new();
    for value in input.into_iter().filter(|row| row["provider"] == "claude") {
        let row: YeloUsageMeter = match serde_json::from_value(value) {
            Ok(row) => row,
            Err(_) => {
                skipped.push("invalid Claude usage row".into());
                continue;
            }
        };
        let account = row.label.split_once('·').and_then(|(_, tail)| {
            account_lists
                .iter()
                .find(|list| list.provider == "claude")
                .and_then(|list| {
                    list.accounts
                        .iter()
                        .find(|account| account.email.as_deref() == Some(tail))
                        .or_else(|| list.accounts.iter().find(|account| account.name == tail))
                })
        });
        let as_of = row.as_of.filter(|time| *time >= 0 && *time <= now);
        let valid_percent = row.pct.is_none_or(|percent| (0..=100).contains(&percent));
        let state = if !valid_percent {
            "failed"
        } else {
            match row.state.as_str() {
                "ok" | "fresh" if row.pct.is_some() => {
                    if as_of.is_some_and(|time| now - time <= crate::usage::MAX_AGE_SECONDS) {
                        "fresh"
                    } else {
                        "stale"
                    }
                }
                "stale" if row.pct.is_some() => "stale",
                "missing" | "logged_out" => "missing",
                _ => "failed",
            }
        };
        let window_minutes = row.window.as_deref().and_then(crate::usage::window_minutes);
        let window = row.window.filter(|_| window_minutes.is_some());
        meters.push(UsageMeter {
            provider: "claude".into(),
            account: account.map(|account| account.name.clone()),
            label: account
                .map(|account| account.name.clone())
                .unwrap_or_else(|| "Claude".into()),
            window,
            window_minutes,
            used_pct: matches!(state, "fresh" | "stale")
                .then_some(row.pct)
                .flatten(),
            reset_time_seconds: None,
            state: state.into(),
            source: Some("yelo".into()),
            reason: match state {
                "failed" => Some("Claude usage read failed".into()),
                "missing" => Some("No cached Claude usage".into()),
                _ => None,
            },
            as_of_seconds: as_of,
        });
    }
    Ok((Usage { meters }, skipped))
}

pub fn resolve_account<'a>(
    accounts: &'a AccountList,
    requested: &str,
) -> Result<&'a Account, String> {
    let name = if requested == "auto" {
        accounts
            .auto
            .as_deref()
            .ok_or_else(|| format!("no automatic account for {}", accounts.provider))?
    } else {
        requested
    };
    let account = accounts
        .accounts
        .iter()
        .find(|account| account.name == name)
        .ok_or_else(|| format!("unknown {} account {name}", accounts.provider))?;
    match account.auth_state {
        AuthState::SignedIn => Ok(account),
        AuthState::SignedOut => Err(format!(
            "{} account {name} is signed out",
            accounts.provider
        )),
        AuthState::Unavailable => Err(format!(
            "{} account {name} authentication is unavailable",
            accounts.provider
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str, state: AuthState, usage: &str, remaining: Option<i64>) -> Account {
        Account {
            name: name.into(),
            email: None,
            home: format!("/profiles/{name}"),
            env: BTreeMap::new(),
            auth_state: state,
            usage_state: usage.into(),
            usage_source: None,
            remaining_pct: remaining,
            summary: None,
        }
    }

    #[test]
    fn auto_uses_fresh_most_left_then_names_and_keeps_current_without_fresh_quota() {
        let mut rows = vec![
            account("work", AuthState::SignedIn, "fresh", Some(70)),
            account("personal", AuthState::SignedIn, "fresh", Some(25)),
            account("spare", AuthState::SignedOut, "fresh", Some(100)),
        ];
        assert_eq!(
            pick_auto(&rows, Some("/profiles/personal")).as_deref(),
            Some("work")
        );
        rows[1].remaining_pct = Some(70);
        assert_eq!(pick_auto(&rows, None).as_deref(), Some("personal"));
        rows[0].usage_state = "stale".into();
        rows[0].remaining_pct = Some(100);
        assert_eq!(pick_auto(&rows, None).as_deref(), Some("personal"));
        rows[1].usage_state = "missing".into();
        rows[1].remaining_pct = None;
        assert_eq!(
            pick_auto(&rows, Some("/profiles/work")).as_deref(),
            Some("work")
        );
        assert_eq!(pick_auto(&rows, None), None);
        rows[0].auth_state = AuthState::Unavailable;
        assert_eq!(pick_auto(&rows, Some("/profiles/work")), None);
    }

    #[test]
    fn named_resolution_rejects_known_signed_out_and_unavailable_authentication() {
        let mut list = empty_accounts("codex");
        list.accounts
            .push(account("work", AuthState::SignedOut, "missing", None));
        assert!(
            resolve_account(&list, "work")
                .unwrap_err()
                .contains("signed out")
        );
        list.accounts[0].auth_state = AuthState::Unavailable;
        assert!(
            resolve_account(&list, "work")
                .unwrap_err()
                .contains("unavailable")
        );
    }
}
