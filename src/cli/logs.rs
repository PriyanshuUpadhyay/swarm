//! CLI logs.

use super::*;

pub(crate) fn claude_chair_log(id: &str) -> Option<std::path::PathBuf> {
    let config = env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".claude"))
        })?;
    std::fs::read_dir(config.join("projects"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(format!("{id}.jsonl")))
        .find(|path| path.is_file())
}

pub(crate) fn codex_chair_log(id: &str, days: &[String; 3]) -> Option<std::path::PathBuf> {
    let home = env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".codex"))
        })?;
    let suffix = format!("-{id}.jsonl");
    days.iter()
        .filter_map(|day| std::fs::read_dir(home.join("sessions").join(day)).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(&suffix))
        })
}

/// A pi session file, `<agent dir>/sessions/--<cwd>--/<time>_<id>.jsonl`. Pi gives each tool
/// command the file of its own session, so that comes first.
pub(crate) fn pi_chair_log(id: &str) -> Option<std::path::PathBuf> {
    let suffix = format!("_{id}.jsonl");
    // The name test comes first, so a scan of every session file stats only the match.
    let named = |path: &std::path::Path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(&suffix))
            && path.is_file()
    };
    if let Some(file) = env::var_os("PI_SESSION_FILE").map(std::path::PathBuf::from)
        && named(&file)
    {
        return Some(file);
    }
    let agent = env::var_os("PI_CODING_AGENT_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".pi/agent"))
        })?;
    std::fs::read_dir(agent.join("sessions"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|dir| std::fs::read_dir(dir.path()).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| named(path))
}

/// The app reads a live chat's log from its session row, and a pi chair's file is known when
/// the chair is named, so it is recorded then.
pub(crate) fn record_pi_log(
    connection: &rusqlite::Connection,
    session: &str,
    chair: Option<(&str, &str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(("pi", id)) = chair
        && let Some(log) = pi_chair_log(id)
    {
        swarm::store::set_chair_log(connection, session, &log)?;
    }
    Ok(())
}

pub(crate) fn resolved_chair_log(row: &swarm::store::SessionRow) -> Option<std::path::PathBuf> {
    if let Some(path) = row.chair_log.as_deref().map(std::path::PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let provider = row.chair_provider.as_deref()?;
    let id = row.chair_id.as_deref().filter(|id| valid_chair_id(id))?;
    match provider {
        "claude" => claude_chair_log(id),
        "codex" => codex_chair_log(id, &row.chair_days),
        "pi" => pi_chair_log(id),
        _ => None,
    }
}

/// An agent's chat log, or the same Codex rollout under `archived_sessions/` once Codex has moved
/// it there from `sessions/<y>/<m>/<d>/`.
pub(crate) fn resolved_agent_log(log: String) -> String {
    let path = std::path::Path::new(&log);
    if path.is_file() {
        return log;
    }
    path.ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "sessions"))
        .and_then(|sessions| {
            Some(
                sessions
                    .parent()?
                    .join("archived_sessions")
                    .join(path.file_name()?),
            )
        })
        .filter(|archived| archived.is_file())
        .map_or(log, |archived| archived.to_string_lossy().into_owned())
}
