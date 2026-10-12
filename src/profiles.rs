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
    #[serde(skip)]
    pub invalid_home: bool,
}

impl Account {
    pub fn invalid_home(&self) -> bool {
        self.invalid_home
    }
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
        revision: crate::config::revision(crate::accounts::BUNDLED.as_bytes()),
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

pub fn apply_usage(list: &mut AccountList, meters: &[UsageMeter]) {
    for account in &mut list.accounts {
        if account.usage_state == "no_source" {
            continue;
        }
        let rows: Vec<_> = meters
            .iter()
            .filter(|meter| {
                meter.provider == list.provider && meter.account.as_deref() == Some(&account.name)
            })
            .collect();
        account.remaining_pct = None;
        account.summary = None;
        account.usage_state = if rows.iter().any(|meter| meter.state == "failed") {
            "failed"
        } else if rows.iter().any(|meter| meter.state == "missing") || rows.is_empty() {
            "missing"
        } else if rows.iter().any(|meter| meter.state == "no_source") {
            "no_source"
        } else if rows.iter().any(|meter| meter.state == "stale") {
            "stale"
        } else {
            "fresh"
        }
        .into();
        if matches!(account.usage_state.as_str(), "fresh" | "stale") {
            account.remaining_pct = rows
                .iter()
                .filter_map(|meter| meter.used_pct.map(|used| 100 - used))
                .min();
            account.summary = account
                .remaining_pct
                .map(|remaining| format!("{remaining}% left"));
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
            invalid_home: false,
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
