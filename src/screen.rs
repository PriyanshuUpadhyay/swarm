//! Read an agent pane's bottom rows last, for states its provider's hooks cannot send or when
//! a hook report is stale or absent, and decide which report to show.

/// How long a hook report is the authority over the screen, in seconds.
pub const HOOK_AUTHORITY_S: i64 = 10;
/// How long an owner's send reads as working with no newer report: the CLI picks the prompt up
/// and sends its first report a second or two later, and its screen still shows the idle prompt.
pub const SEND_HOLD_S: i64 = 5;
/// A `working` or `waiting` report older than this, in seconds, that nothing confirms is shown
/// as no report.
pub const STALE_S: i64 = 45;
/// How many bottom rows of a pane the classifier reads.
pub const ROWS: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    /// A spinner or an interrupt hint shows a turn in progress.
    Working,
    Waiting,
    Idle,
    /// The turn ended on an error row, such as Codex's red `■ message`.
    Failed,
}

impl ScreenState {
    pub fn as_state(self) -> &'static str {
        match self {
            ScreenState::Working => "working",
            ScreenState::Waiting => "waiting",
            ScreenState::Idle => "done",
            ScreenState::Failed => "failed",
        }
    }
}

mod classify;
mod composer;
mod live;
mod log_prompt;
mod prompt;

pub use classify::{failure_detail, herdr_state, read_pane, screen_state};
pub use composer::{
    QueuedMessage, claude_composer, codex_composer, composer, holds, pi_composer, pi_queue, queue,
    reads_composer,
};
pub use live::turn_text;
pub use log_prompt::agent_prompt;
pub use prompt::{Prompt, prompt, whole_prompt};

/// The states a provider's hooks cannot send. Only these may let a screen read overturn a hook
/// report younger than `STALE_S`, after the `HOOK_AUTHORITY_S` window.
pub fn screen_gap(provider: Option<&str>, screen: ScreenState) -> bool {
    matches!(
        (provider, screen),
        (Some("codex"), ScreenState::Failed)
            | (Some("agy"), ScreenState::Waiting)
            | (_, ScreenState::Idle)
    )
}

/// Whether a send at `sent_at` shows the agent as working now: no report came after it, it is
/// younger than `SEND_HOLD_S`, and the screen shows the idle prompt or nothing. A question or a
/// failure on the screen still shows at once.
pub fn held_by_send(
    sent_at: Option<i64>,
    state_at: Option<i64>,
    screen: Option<ScreenState>,
    now: i64,
) -> bool {
    sent_at.is_some_and(|sent| {
        now - sent < SEND_HOLD_S
            && state_at.is_none_or(|at| at < sent)
            && screen.is_none_or(|screen| screen == ScreenState::Idle)
    })
}

/// What the listing shows for one agent, and the state to write, if any.
///
/// A hook report younger than `HOOK_AUTHORITY_S` wins. Until `STALE_S`, only a state in
/// `screen_gap` may replace it. Then a differing screen result is written with source `screen`.
/// An idle screen agrees with a hook's `failed`, because a failed
/// turn also ends at the idle prompt, but not with a `failed` the screen wrote. A `working` or `waiting` report older than `STALE_S` that the
/// screen does not confirm is shown as no report, and nothing is written for it.
pub fn resolve(
    provider: Option<&str>,
    state: Option<&str>,
    state_at: Option<i64>,
    source: Option<&str>,
    screen: Option<ScreenState>,
    now: i64,
) -> (Option<String>, Option<&'static str>) {
    let age = state_at.map(|at| now - at);
    // A stored host record reaches here only when its read failed, and an agent's own report
    // stands over the screen the same way, whether a hook or its host gave it (ADR 0070).
    let own = matches!(source, Some("hook" | "host"));
    if own && age.is_some_and(|age| age < HOOK_AUTHORITY_S) {
        return (state.map(str::to_string), None);
    }
    if own
        && age.is_some_and(|age| age <= STALE_S)
        && screen.is_some_and(|screen| !screen_gap(provider, screen))
    {
        return (state.map(str::to_string), None);
    }
    if let Some(screen) = screen {
        let seen = screen.as_state();
        // A failed turn also ends at the idle prompt, so an idle screen agrees with a failure a
        // hook reported. A failure the screen itself saw lasts only while its error row shows.
        let agrees = state == Some(seen)
            || (seen == "done" && state == Some("failed") && source != Some("screen"));
        if !agrees {
            return (Some(seen.to_string()), Some(seen));
        }
        return (state.map(str::to_string), None);
    }
    let stale = matches!(state, Some("working" | "waiting")) && age.is_none_or(|age| age > STALE_S);
    (state.filter(|_| !stale).map(str::to_string), None)
}

#[cfg(test)]
macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("../../tests/fixtures/screens/", $name, ".txt"))
    };
}

#[cfg(test)]
mod classify_tests;
#[cfg(test)]
mod composer_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod report_tests;
