use crate::managed::{
    Conflict, ConflictKind, Edit, FilePlan, Kind, Writer, json_object, json_text, read_json_object,
    read_optional, read_text, refuse_read_only, retried,
};
use crate::providers::Provider;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ResolvedRole {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub sandbox: Option<String>,
    pub approval: Option<String>,
    pub permission: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Agent {
    pub id: String,
    pub role: String,
    pub pane: Option<String>,
    pub provider: Option<String>,
    pub created_at: i64,
    pub alive: Option<bool>,
    /// `working`, `waiting`, `done`, or `failed`; null until the first report (ADR 0021).
    pub state: Option<String>,
    pub state_at_s: Option<i64>,
    /// `hook`, `host` (the host terminal's program-status record), or `screen`.
    pub state_source: Option<String>,
    pub state_detail: Option<String>,
    pub state_kind: Option<String>,
    /// The chat log the agent's provider hooks reported; null until the first report.
    pub log: Option<String>,
    /// The question the agent's screen shows now, if any; `swarm answer` picks a choice.
    pub prompt: Option<crate::screen::Prompt>,
    /// The messages the agent's CLI holds for a later turn, as its screen lists them; absent when
    /// swarm does not read that CLI's queue from the screen (ADR 0130).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<crate::screen::QueuedMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(rename = "costUsd", skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AgentList {
    pub agents: Vec<Agent>,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub seq: i64,
    pub sender: String,
    pub recipient: String,
    pub kind: String,
    pub body: Option<String>,
    pub created_at: i64,
    pub read: bool,
    /// What the last ring proved: `hook`, `screen`, `seen`, `unconfirmed`, or `unchecked`; null until a
    /// ring ends. More values may come (ADR 0041).
    pub delivery: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageList {
    pub messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: String,
    pub talk_mode: String,
    pub adapter: Option<String>,
    pub cwd: String,
    pub created_at: i64,
    pub chair_provider: Option<String>,
    pub chair_id: Option<String>,
    pub chair_log: Option<String>,
    pub continuation_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_of: Option<String>,
    pub agents: i64,
    pub messages: i64,
    pub last_message_at: Option<i64>,
    #[serde(rename = "archivedAt", skip_serializing_if = "Option::is_none")]
    pub archived_at_s: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionList {
    pub sessions: Vec<Session>,
}

/// The folder that holds tmux's sockets, as tmux names it: `$TMUX_TMPDIR` or `/tmp`, resolved,
/// then `tmux-<uid>`.
fn tmux_socket_dir(tmpdir: Option<&str>, uid: u32) -> String {
    let base = tmpdir.filter(|dir| !dir.is_empty()).unwrap_or("/tmp");
    let base = std::fs::canonicalize(base)
        .map_or(base.to_string(), |path| path.to_string_lossy().into_owned());
    format!("{base}/tmux-{uid}")
}

mod codex;
mod hooks;
mod launch;
mod trust;

pub use codex::*;
pub use hooks::*;
pub use launch::*;
pub use trust::*;

/// A Codex `config.toml` as text and as TOML.
fn read_codex_config(path: &std::path::Path) -> Result<(String, toml_edit::DocumentMut), String> {
    let existing = read_text(path)?;
    let config = existing.parse().map_err(|error| {
        format!(
            "{} is not valid TOML, so swarm does not edit it: {error}",
            path.display()
        )
    })?;
    Ok((existing, config))
}

/// The timeout of each state hook that swarm passes or writes, in seconds.
const STATE_HOOK_TIMEOUT: u64 = 3;

/// The table `key` of `parent`, added when it is missing. An added `implicit` table writes no
/// header of its own. None when `key` holds a value that is not a table.
fn toml_table<'a>(
    parent: &'a mut dyn toml_edit::TableLike,
    key: &str,
    implicit: bool,
) -> Option<&'a mut dyn toml_edit::TableLike> {
    if parent.get(key).is_none() {
        let mut table = toml_edit::Table::new();
        table.set_implicit(implicit);
        parent.insert(key, toml_edit::Item::Table(table));
    }
    parent.get_mut(key)?.as_table_like_mut()
}

/// This executable's path, single-quoted for a shell command line.
fn quoted_exe() -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("swarm: cannot find executable: {error}"))?
        .to_string_lossy()
        .replace('\'', "'\\''");
    Ok(format!("'{exe}'"))
}

fn chair_hook_command(provider: &str) -> Result<String, String> {
    let exe = quoted_exe()?;
    // Both CLI SessionStart payloads include a top-level session_id string.
    Ok(format!(
        "id=$(sed -n 's/.*\"session_id\"[[:space:]]*:[[:space:]]*\"\\([A-Za-z0-9-]\\{{1,64\\}}\\)\".*/\\1/p'); [ -n \"$id\" ] && {exe} session chair {provider}:\"$id\""
    ))
}

const CLAUDE_STATE_EVENTS: [&str; 8] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Elicitation",
    "Notification",
    "Stop",
    "StopFailure",
];

const CODEX_STATE_EVENTS: [&str; 6] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "Interrupt",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::{changed_error, with_lock};

    fn role(provider: &str) -> ResolvedRole {
        ResolvedRole {
            provider: Some(provider.into()),
            model: Some("model-1".into()),
            effort: Some("high".into()),
            sandbox: None,
            approval: None,
            permission: None,
        }
    }

    /// Setup for one file as `swarm setup hooks` does it: no write while the file has a conflict.
    /// Apply `plan` through the module with a store of its own; whether the file changed.
    fn set_up(plan: Result<FilePlan, String>) -> Result<bool, String> {
        apply(&[plan?]).map(|changed| !changed.is_empty())
    }

    fn apply(plans: &[FilePlan]) -> Result<Vec<std::path::PathBuf>, String> {
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        crate::managed::apply(
            &store,
            &plans.iter().cloned().map(Into::into).collect::<Vec<_>>(),
        )
    }

    /// A launch's trust write as `write_trust` makes it, with a store of its own; whether the
    /// file changed.
    fn trusted(plan: impl Fn() -> Result<FilePlan, String>) -> Result<bool, String> {
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        write_trust(&store, plan).map(|plan| plan.is_some())
    }

    fn ensure_codex_trust(home: &std::path::Path, dir: &std::path::Path) -> Result<bool, String> {
        trusted(|| codex_trust_plan(home, dir))
    }

    fn ensure_claude_trust(
        config: &std::path::Path,
        dir: &std::path::Path,
    ) -> Result<bool, String> {
        trusted(|| claude_trust_plan(config, dir))
    }

    fn ensure_agy_trust(settings: &std::path::Path, dir: &std::path::Path) -> Result<bool, String> {
        trusted(|| agy_trust_plan(settings, dir))
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    mod hooks;
    mod launch;
    mod policy;
    mod trust;
    mod trust_targets;
}
