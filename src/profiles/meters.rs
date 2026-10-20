// Usage meter types and reason codes.
use super::*;

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
    /// Codex only: the account's usage limit resets, on each of its window meters. An older
    /// cache has no field, which reads as none.
    #[serde(default)]
    pub reset_credits: Option<ResetCredits>,
}

/// Usage limit resets that ChatGPT granted a Codex account (`docs/usage-format.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResetCredits {
    pub available: i64,
    /// One entry per listed credit, soonest first, a credit that never expires (null) last. Null
    /// when the reply had no list; the list can be shorter than `available`.
    pub expiry_time_seconds: Option<Vec<Option<i64>>>,
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
