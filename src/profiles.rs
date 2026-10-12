use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The version of the printed account format (`docs/usage-format.md`).
pub const ACCOUNTS_SCHEMA: &str = "swarm.accounts.v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountList {
    pub provider: String,
    pub source: Option<String>,
    pub state: String,
    pub revision: String,
    pub modified: bool,
    pub accounts: Vec<Account>,
    pub auto: Option<String>,
    #[serde(skip)]
    pub current_home: Option<String>,
    #[serde(skip)]
    pub meters: Vec<UsageMeter>,
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
    #[serde(skip)]
    pub fable_spent: bool,
    /// Signed in with an expired token, which the CLI renews when a chat starts.
    #[serde(skip)]
    pub renewable: bool,
}

impl AccountList {
    pub fn for_model(&self, model: Option<&str>) -> Self {
        let mut list = self.clone();
        apply_usage_for_model(&mut list, &self.meters, model);
        list
    }
}

/// `best` is the Claude CLI alias for its most capable model, which is Fable, so a chat on `best`
/// spends the Fable limit and the pick must count it.
pub fn is_fable_model(model: Option<&str>) -> bool {
    model.is_some_and(|model| {
        let model = model.to_ascii_lowercase();
        model.contains("fable") || model == "best"
    })
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
        current_home: None,
        meters: Vec::new(),
    }
}

/// The fresh account with the most left when it has at least `min_usage_left_pct`, else the
/// first renewable account by name, else the fresh account with the most left, else the current
/// home. A signed-out account is never picked.
pub fn pick_auto(
    accounts: &[Account],
    current_home: Option<&str>,
    model: Option<&str>,
    min_usage_left_pct: u8,
) -> Result<Option<String>, String> {
    let fable = is_fable_model(model);
    let signed_in: Vec<_> = accounts
        .iter()
        .filter(|account| account.auth_state == AuthState::SignedIn)
        .collect();
    if fable && !signed_in.is_empty() && signed_in.iter().all(|account| account.fable_spent) {
        return Err("every signed-in Claude account has used its Fable limit".into());
    }
    let signed_in = || {
        accounts
            .iter()
            .filter(|account| account.auth_state == AuthState::SignedIn)
            .filter(|account| !fable || !account.fable_spent)
    };
    let fresh = signed_in()
        .filter(|account| account.usage_state == "fresh" && account.remaining_pct.is_some())
        .min_by(|left, right| {
            right
                .remaining_pct
                .cmp(&left.remaining_pct)
                .then(left.name.cmp(&right.name))
        });
    Ok(fresh
        .filter(|account| account.remaining_pct >= Some(i64::from(min_usage_left_pct)))
        .or_else(|| {
            signed_in()
                .filter(|account| account.renewable)
                .min_by(|left, right| left.name.cmp(&right.name))
        })
        .or(fresh)
        .or_else(|| signed_in().find(|account| Some(account.home.as_str()) == current_home))
        .map(|account| account.name.clone()))
}

pub fn apply_usage(list: &mut AccountList, meters: &[UsageMeter]) {
    list.meters = meters.to_vec();
    apply_usage_for_model(list, meters, None);
}

fn apply_usage_for_model(list: &mut AccountList, meters: &[UsageMeter], model: Option<&str>) {
    let fable = list.provider == "claude" && is_fable_model(model);
    let now = crate::usage::now_seconds();
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
        account.fable_spent = list.provider == "claude"
            && rows.iter().any(|meter| {
                meter.window.as_deref() == Some("fb")
                    && matches!(meter.state.as_str(), "fresh" | "stale")
                    && meter.used_pct == Some(100)
                    && meter.reset_time_seconds.is_none_or(|reset| reset > now)
            });
        account.renewable = list.provider == "claude"
            && account.auth_state == AuthState::SignedIn
            && rows.iter().any(|meter| {
                meter.state == "failed" && meter.reason_code == Some(ReasonCode::TokenExpired)
            });
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|meter| {
                list.provider != "claude"
                    || meter.window.as_deref() != Some("fb")
                    || (fable && meter.reset_time_seconds.is_none_or(|reset| reset > now))
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
    /// Set exactly when `reason` is; a reader matches this, never the text.
    pub reason_code: Option<ReasonCode>,
    pub as_of_seconds: Option<i64>,
}

impl UsageMeter {
    /// `text` names the cause in more detail than the code's own text; only `no_source` and
    /// `read_failed` take one.
    pub(crate) fn set_reason(&mut self, code: ReasonCode, text: Option<&str>) {
        self.reason = Some(text.unwrap_or(code.text()).into());
        self.reason_code = Some(code);
    }

    pub(crate) fn clear_reason(&mut self) {
        self.reason = None;
        self.reason_code = None;
    }
}

/// Why a meter has no current reading, with its text (`docs/usage-format.md`). New codes may
/// appear; a reader shows `reason` for one it does not know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// The CLI renews the token when a chat starts, so Auto may still pick the account.
    TokenExpired,
    ReloginRequired,
    NoCredentials,
    RefreshFailed,
    NoCachedUsage,
    NoSource,
    ReadFailed,
    NoQuotaWindows,
}

impl ReasonCode {
    pub const ALL: [ReasonCode; 8] = [
        ReasonCode::TokenExpired,
        ReasonCode::ReloginRequired,
        ReasonCode::NoCredentials,
        ReasonCode::RefreshFailed,
        ReasonCode::NoCachedUsage,
        ReasonCode::NoSource,
        ReasonCode::ReadFailed,
        ReasonCode::NoQuotaWindows,
    ];

    pub fn text(self) -> &'static str {
        match self {
            ReasonCode::TokenExpired => "Token expired",
            ReasonCode::ReloginRequired => "Sign in again",
            ReasonCode::NoCredentials => "Account is signed out",
            ReasonCode::RefreshFailed => "Usage refresh failed; old reading",
            ReasonCode::NoCachedUsage => "No cached usage",
            ReasonCode::NoSource => "No usage source",
            ReasonCode::ReadFailed => "Usage read failed",
            ReasonCode::NoQuotaWindows => "No Codex quota windows",
        }
    }
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
            fable_spent: false,
            renewable: false,
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
            pick_auto(&rows, Some("/profiles/personal"), None, 5)
                .unwrap()
                .as_deref(),
            Some("work")
        );
        rows[1].remaining_pct = Some(70);
        assert_eq!(
            pick_auto(&rows, None, None, 5).unwrap().as_deref(),
            Some("personal")
        );
        rows[0].usage_state = "stale".into();
        rows[0].remaining_pct = Some(100);
        assert_eq!(
            pick_auto(&rows, None, None, 5).unwrap().as_deref(),
            Some("personal")
        );
        rows[1].usage_state = "missing".into();
        rows[1].remaining_pct = None;
        assert_eq!(
            pick_auto(&rows, Some("/profiles/work"), None, 5)
                .unwrap()
                .as_deref(),
            Some("work")
        );
        assert_eq!(pick_auto(&rows, None, None, 5).unwrap(), None);
        rows[0].auth_state = AuthState::Unavailable;
        assert_eq!(
            pick_auto(&rows, Some("/profiles/work"), None, 5).unwrap(),
            None
        );
    }

    fn meter(name: &str, state: &str, used: Option<i64>, code: Option<ReasonCode>) -> UsageMeter {
        UsageMeter {
            provider: "claude".into(),
            account: Some(name.into()),
            label: name.into(),
            window: used.map(|_| "5h".into()),
            window_minutes: None,
            used_pct: used,
            reset_time_seconds: None,
            state: state.into(),
            source: None,
            reason: code.map(|code| code.text().into()),
            reason_code: code,
            as_of_seconds: None,
        }
    }

    fn claude_list(meters: &[UsageMeter]) -> AccountList {
        let mut list = empty_accounts("claude");
        for name in ["alpha", "bravo", "charlie"] {
            list.accounts
                .push(account(name, AuthState::SignedIn, "missing", None));
        }
        apply_usage(&mut list, meters);
        list
    }

    #[test]
    fn auto_takes_a_renewable_account_only_below_the_threshold_and_never_a_signed_out_one() {
        let expired = |name| meter(name, "failed", None, Some(ReasonCode::TokenExpired));
        let pick = |list: &AccountList| pick_auto(&list.accounts, None, None, 5).unwrap();
        let list = claude_list(&[meter("alpha", "fresh", Some(60), None), expired("bravo")]);
        assert!(list.accounts[1].renewable);
        assert_eq!(pick(&list).as_deref(), Some("alpha"));
        let list = claude_list(&[meter("alpha", "fresh", Some(97), None), expired("bravo")]);
        assert_eq!(pick(&list).as_deref(), Some("bravo"));
        let list = claude_list(&[expired("charlie"), expired("bravo")]);
        assert_eq!(pick(&list).as_deref(), Some("bravo"));
        let mut list = claude_list(&[expired("bravo")]);
        list.accounts[1].auth_state = AuthState::SignedOut;
        assert_eq!(pick(&list), None);
        apply_usage(&mut list, &[expired("bravo")]);
        assert!(!list.accounts[1].renewable);
        let list = claude_list(&[meter(
            "bravo",
            "failed",
            None,
            Some(ReasonCode::ReloginRequired),
        )]);
        assert!(!list.accounts[1].renewable);
        assert_eq!(pick(&list), None);
        let fable = claude_list(&[expired("bravo")]).for_model(Some("fable"));
        assert!(!fable.accounts[1].fable_spent);
        assert_eq!(
            pick_auto(&fable.accounts, None, Some("fable"), 5),
            Ok(Some("bravo".into()))
        );
    }

    #[test]
    fn the_spec_lists_each_reason_code_with_its_text_and_no_other() {
        let spec = include_str!("../docs/usage-format.md");
        let section = spec.split("## Reason codes").nth(1).unwrap();
        let section = section.split("\n## ").next().unwrap();
        let rows: Vec<_> = section
            .lines()
            .filter(|line| line.starts_with("| `"))
            .collect();
        let expected: Vec<_> = ReasonCode::ALL
            .iter()
            .map(|code| {
                let name = serde_json::to_value(code).unwrap();
                format!("| `{}` | {} |", name.as_str().unwrap(), code.text())
            })
            .collect();
        assert_eq!(rows.len(), expected.len(), "{rows:#?}");
        for (row, expected) in rows.iter().zip(&expected) {
            assert!(
                row.starts_with(expected.as_str()),
                "{row} is not {expected}"
            );
        }
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

    fn model_list(provider: &str, fb: Option<i64>, state: &str, reset: Option<i64>) -> AccountList {
        let mut list = empty_accounts(provider);
        list.accounts
            .push(account("work", AuthState::SignedIn, "missing", None));
        list.current_home = Some("/profiles/work".into());
        let general = UsageMeter {
            provider: provider.into(),
            account: Some("work".into()),
            label: "work".into(),
            window: Some("5h".into()),
            window_minutes: Some(300),
            used_pct: Some(10),
            reset_time_seconds: None,
            state: "fresh".into(),
            source: None,
            reason: None,
            reason_code: None,
            as_of_seconds: None,
        };
        let fable = UsageMeter {
            window: Some("fb".into()),
            used_pct: fb,
            state: state.into(),
            reset_time_seconds: reset,
            ..general.clone()
        };
        apply_usage(&mut list, &[general, fable]);
        list
    }

    #[test]
    fn fable_auto_refuses_the_only_signed_in_account_with_a_valid_spent_sample() {
        for state in ["fresh", "stale"] {
            let list = model_list("claude", Some(100), state, None).for_model(Some("best"));
            assert_eq!(
                pick_auto(
                    &list.accounts,
                    list.current_home.as_deref(),
                    Some("best"),
                    5
                ),
                Err("every signed-in Claude account has used its Fable limit".into())
            );
            assert!(resolve_account(&list, "work").is_ok());
        }
    }

    #[test]
    fn fable_auto_keeps_unknown_and_reset_samples_usable() {
        let now = crate::usage::now_seconds();
        for (used, state, reset) in [
            (None, "missing", None),
            (None, "failed", None),
            (Some(100), "failed", None),
            (Some(100), "fresh", Some(now - 1)),
            (Some(100), "fresh", Some(now)),
        ] {
            let list = model_list("claude", used, state, reset).for_model(Some("fable"));
            assert!(!list.accounts[0].fable_spent);
            assert_eq!(
                pick_auto(
                    &list.accounts,
                    list.current_home.as_deref(),
                    Some("fable"),
                    5
                ),
                Ok(Some("work".into()))
            );
        }
    }

    #[test]
    fn fable_auto_with_no_signed_in_account_has_no_pick() {
        assert_eq!(pick_auto(&[], None, Some("fable"), 5), Ok(None));
        let mut list = model_list("claude", Some(100), "fresh", None).for_model(Some("fable"));
        list.accounts[0].auth_state = AuthState::SignedOut;
        assert_eq!(pick_auto(&list.accounts, None, Some("fable"), 5), Ok(None));
    }

    #[test]
    fn fable_windows_affect_only_claude_fable_models_and_stay_out_of_json() {
        let list = model_list("claude", Some(80), "fresh", None);
        for model in ["FABLE", "claude-fable-5-1", "best", "BEST"] {
            let scoped = list.for_model(Some(model));
            assert_eq!(scoped.accounts[0].remaining_pct, Some(20));
        }
        for model in [None, Some("sonnet"), Some("opus")] {
            assert_eq!(list.for_model(model).accounts[0].remaining_pct, Some(90));
        }
        let failed = model_list("claude", None, "failed", None);
        assert_eq!(failed.accounts[0].usage_state, "fresh");
        let codex = model_list("codex", Some(100), "fresh", None).for_model(Some("fable"));
        assert_eq!(codex.accounts[0].remaining_pct, Some(0));
        assert_eq!(
            pick_auto(&codex.accounts, None, Some("fable"), 5),
            Ok(Some("work".into()))
        );
        let json = serde_json::to_value(&list).unwrap();
        assert!(json.get("meters").is_none());
        assert!(json.get("current_home").is_none());
        assert!(json["accounts"][0].get("fable_spent").is_none());
    }
}
