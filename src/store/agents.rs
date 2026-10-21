//! Agent records, panes, and provider settings.

use super::*;

pub fn add_agent(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "INSERT INTO agent (id, session_id, role) VALUES (?1, ?2, ?3)",
        (agent_id, session_id, role),
    )?;
    Ok(())
}

pub fn remove_agent(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "DELETE FROM agent WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id),
    )?;
    Ok(())
}

pub fn set_provider(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    provider: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET provider = ?3 WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id, provider),
    )?;
    Ok(())
}

/// **An empty pane is refused here rather than stored.** A stale adapter answered `self` with
/// nothing, `add_agent` wrote that nothing, and the chat said `no pane recorded` at the first
/// message instead of at registration. The guard is on the write because every caller can be
/// handed an empty answer by its adapter, `spawn_agent` included.
pub fn set_pane(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    pane_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if pane_id.trim().is_empty() {
        return Err(format!("swarm: adapter gave no pane for {agent_id}").into());
    }
    let moves_orchestrator =
        orchestrator_of(connection, session_id).is_ok_and(|orchestrator| orchestrator == agent_id);
    connection.execute(
        "UPDATE agent AS existing
         SET pane_id = CASE WHEN existing.session_id = ?2 AND existing.id = ?3 THEN ?1 ELSE NULL END
         WHERE (existing.session_id = ?2 AND existing.id = ?3)
            OR (?4 AND existing.pane_id = ?1 AND existing.session_id != ?2 AND EXISTS (
                SELECT 1 FROM session AS previous
                JOIN session AS target ON target.id = ?2
                WHERE previous.id = existing.session_id
                  AND previous.adapter IS target.adapter
            ))",
        (pane_id, session_id, agent_id, moves_orchestrator),
    )?;
    Ok(())
}

/// The session and orchestrator whose pane this is. `set_pane` keeps an orchestrator's pane in one
/// session of an adapter, so only a pane shared by two adapters needs the newest-session order.
pub fn orchestrator_at_pane(
    connection: &Connection,
    pane_id: &str,
) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row(
            "SELECT agent.session_id, agent.id FROM agent
             JOIN session ON session.id = agent.session_id
             WHERE agent.pane_id = ?1
               AND (agent.role = 'orchestrator' OR agent.id = 'orchestrator')
               AND session.archived_at IS NULL
             ORDER BY session.created_at DESC, session.id DESC LIMIT 1",
            [pane_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

pub fn pane_of(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let pane: Option<String> = connection.query_row(
        "SELECT pane_id FROM agent WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id),
        |r| r.get(0),
    )?;
    Ok(pane)
}

/// The pane of an agent a command acts on. Unlike `pane_of`, an agent the session does not hold is
/// an error that names it, not the raw "Query returned no rows".
pub fn require_pane(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use rusqlite::OptionalExtension;
    let row: Option<Option<String>> = connection
        .query_row(
            "SELECT pane_id FROM agent WHERE session_id = ?1 AND id = ?2",
            (session_id, agent_id),
            |r| r.get(0),
        )
        .optional()?;
    match row {
        None => Err(format!("swarm: no agent {agent_id} in this session").into()),
        Some(pane) => pane.ok_or_else(|| format!("swarm: no pane recorded for {agent_id}").into()),
    }
}

/// The CLI that runs in an agent's pane. A chair's row names none, so it is the session's
/// `chair_provider`.
/// How long a `working` or `waiting` report counts without a newer one. A turn reports at each tool
/// call, and an agent that died keeps its last report, so the usage HUD stops showing it after this.
const WORKING_FOR_S: i64 = 600;

/// The number of agents with a pane that report `state` (`working` or `waiting`), by provider; a
/// chair with no provider of its own counts under its session's chair provider.
pub fn agents_by_provider(
    connection: &Connection,
    state: &str,
) -> Result<std::collections::BTreeMap<String, i64>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT provider, COUNT(*) FROM (
             SELECT COALESCE(agent.provider,
                             CASE WHEN agent.role = 'orchestrator' OR agent.id = 'orchestrator'
                                  THEN session.chair_provider END) AS provider
             FROM agent JOIN session ON session.id = agent.session_id
             WHERE agent.pane_id IS NOT NULL AND agent.state = ?2
               AND agent.state_at >= unixepoch() - ?1)
         WHERE provider IS NOT NULL GROUP BY provider",
    )?;
    let rows = statement.query_map((WORKING_FOR_S, state), |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn provider_of(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let provider: Option<String> = connection.query_row(
        "SELECT COALESCE(agent.provider,
                         CASE WHEN agent.role = 'orchestrator' OR agent.id = 'orchestrator'
                              THEN session.chair_provider END)
         FROM agent JOIN session ON session.id = agent.session_id
         WHERE agent.session_id = ?1 AND agent.id = ?2",
        (session_id, agent_id),
        |r| r.get(0),
    )?;
    Ok(provider)
}

#[derive(Debug)]
pub struct AgentRow {
    pub id: String,
    pub role: String,
    pub pane: Option<String>,
    pub provider: Option<String>,
    pub created_at: i64,
    pub state: Option<String>,
    pub state_at: Option<i64>,
    pub state_source: Option<String>,
    pub state_detail: Option<String>,
    pub state_kind: Option<String>,
    /// When the owner last sent it a prompt (`mark_sent`).
    pub sent_at: Option<i64>,
    /// The chat log its provider's hooks reported; kept after the agent ends.
    pub log: Option<String>,
    pub profile: Option<String>,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub account: Option<String>,
    pub cost_usd: Option<f64>,
    pub tokens: Option<i64>,
}

/// Store the runner selected by launch; absent account means the provider's own login.
pub fn set_launch(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    profile: &str,
    runner: &crate::config::Runner,
    account: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET profile = ?3, runner = ?4, model = ?5, effort = ?6, account = ?7
         WHERE session_id = ?1 AND id = ?2",
        (
            session_id,
            agent_id,
            profile,
            runner.label(),
            &runner.model,
            &runner.effort,
            account,
        ),
    )?;
    Ok(())
}

/// Record the chat log a provider hook reported for an agent.
pub fn set_log(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    log: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Keep each log's cursor across path changes; claude_usage resets only a log that shrinks.
    connection.execute(
        "UPDATE agent SET log = ?3 WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id, log.to_string_lossy()),
    )?;
    Ok(())
}

pub fn agents(
    connection: &Connection,
    session_id: &str,
) -> Result<Vec<AgentRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT id, role, pane_id, provider, created_at, state, state_at, state_source, state_detail, state_kind,
                log, profile, runner, model, effort, account, cost_usd, tokens, sent_at
         FROM agent WHERE session_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([session_id], |row| {
        Ok(AgentRow {
            id: row.get(0)?,
            role: row.get(1)?,
            pane: row.get(2)?,
            provider: row.get(3)?,
            created_at: row.get(4)?,
            state: row.get(5)?,
            state_at: row.get(6)?,
            state_source: row.get(7)?,
            state_detail: row.get(8)?,
            state_kind: row.get(9)?,
            log: row.get(10)?,
            profile: row.get(11)?,
            runner: row.get(12)?,
            model: row.get(13)?,
            effort: row.get(14)?,
            account: row.get(15)?,
            cost_usd: row.get(16)?,
            tokens: row.get(17)?,
            sent_at: row.get(18)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn clear_pane(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        // A closed agent reports nothing, so it shows as ended rather than its last state.
        "UPDATE agent SET pane_id = NULL, state = NULL, state_at = NULL, state_source = NULL,
         state_detail = NULL, state_kind = NULL WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id),
    )?;
    Ok(())
}

/// Session agents that have a pane, except the caller, as (agent_id, pane_id) by id.
pub fn live_children(
    connection: &Connection,
    session_id: &str,
    except: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT id, pane_id FROM agent WHERE session_id = ?1 AND pane_id IS NOT NULL AND id != ?2 ORDER BY id",
    )?;
    let rows = statement.query_map((session_id, except), |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn orchestrator_of(
    connection: &Connection,
    session_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let id = connection
        .query_row(
            "SELECT id FROM agent
             WHERE session_id = ?1 AND (role = 'orchestrator' OR id = 'orchestrator')
             ORDER BY id = 'orchestrator' DESC LIMIT 1",
            [session_id],
            |r| r.get(0),
        )
        .map_err(|_| format!("session {session_id} has no orchestrator"))?;
    Ok(id)
}
