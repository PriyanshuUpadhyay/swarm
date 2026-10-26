//! The session a chat's resume starts again, and the Codex session it moves to another account.

use super::*;

/// Copies the Codex rollout at `rollout` under `from` to the same place under `to`, through the
/// record of writes outside the swarm home (ADR 0042). Codex finds a session by its id in any
/// home that holds its file (probe 2026-10-25), and the newest copy is the chat
/// (`codex_rollout`). A copy that an earlier move left there is replaced.
pub(crate) fn move_codex_session(
    connection: &rusqlite::Connection,
    from: &str,
    rollout: &std::path::Path,
    to: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use swarm::managed::{Edit, Kind, Writer};
    let relative = rollout
        .strip_prefix(from)
        .map_err(|_| format!("swarm: {} is not under {from}", rollout.display()))?;
    let path = std::path::Path::new(to).join(relative);
    let text = std::fs::read_to_string(rollout)?;
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    let mut edit = Edit::new(
        Writer::LaunchCodexMove,
        &path,
        Kind::File,
        &[],
        swarm::managed::file_hash(&text),
    );
    edit.before = (!before.is_empty()).then(|| swarm::managed::file_hash(&before));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let plan = swarm::managed::FilePlan {
        path,
        before,
        after: text,
        conflicts: Vec::new(),
        edits: vec![edit],
    };
    swarm::managed::with_lock(&swarm::paths::trust_lock()?, || {
        swarm::managed::apply(connection, &[plan.into()]).map(|_| ())
    })?;
    Ok(())
}

/// The args that start session `id` of `kind` again. The model and effort flags come before them
/// from the run, so a switch resumes on its new model.
pub(crate) fn resume_args(kind: Provider, id: &str) -> Vec<String> {
    match kind {
        Provider::Codex => vec!["resume".into(), id.into()],
        Provider::Agy => vec!["--conversation".into(), id.into()],
        // pi's launch passes the resumed id as `--session-id`, which opens that session.
        Provider::Pi => Vec::new(),
        Provider::Claude => vec!["--resume".into(), id.into()],
    }
}

/// The CLI and its session id that `launch orchestrator <role> --resume` starts again in this
/// chat, while its chair has no pane.
pub(crate) fn resume_target(
    connection: &rusqlite::Connection,
    session: &str,
) -> Result<(Provider, String), Box<dyn std::error::Error>> {
    let (provider, id, pane): (Option<String>, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT s.chair_provider, s.chair_id, a.pane_id FROM session s
             LEFT JOIN agent a ON a.session_id = s.id AND a.id = 'orchestrator'
             WHERE s.id = ?1",
            [session],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    if pane.is_some() {
        return Err("swarm: this chat's agent is still running".into());
    }
    let provider = provider
        .as_deref()
        .and_then(Provider::parse)
        .ok_or("swarm: this chat's CLI is unknown")?;
    let id = id.ok_or("swarm: this chat has no session id to resume")?;
    Ok((provider, id))
}
