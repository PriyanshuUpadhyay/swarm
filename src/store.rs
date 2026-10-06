use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

pub fn open(path: &Path) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let mut connection = rusqlite::Connection::open(path)?;

    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    // Before WAL, which writes the db header: another program's db is refused unchanged.
    migrate(&mut connection)?;
    connection.execute_batch("PRAGMA journal_mode=WAL;")?;
    connection.pragma_update(None, "foreign_keys", true)?;

    Ok(connection)
}

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001.sql"),
    include_str!("../migrations/0002.sql"),
    include_str!("../migrations/0003.sql"),
    include_str!("../migrations/0004.sql"),
    include_str!("../migrations/0005.sql"),
    include_str!("../migrations/0006.sql"),
];

fn known_version(version: i64) -> bool {
    (1..=MIGRATIONS.len() as i64).contains(&version)
}

/// Whether `path` is a db that a swarm made: it shows a known `user_version` and every table of
/// migration 0001. A home from before the marker is adopted on this (ADR 0036). The open is
/// `immutable`, because a plain read-only open of a WAL db makes `-shm` and `-wal` files in a
/// folder that may not be swarm's. It skips changes still in a `-wal`; swarm's schema was
/// checkpointed long ago, and a miss is a refusal whose message gives the fix.
pub fn made_by_swarm(path: &Path) -> bool {
    // An absolute path after `file://`: a leading `//` would else read as a URI authority.
    let Ok(path) = std::path::absolute(path) else {
        return false;
    };
    let escaped = path
        .to_string_lossy()
        .replace('%', "%25")
        .replace('?', "%3f")
        .replace('#', "%23");
    let Ok(connection) = Connection::open_with_flags(
        format!("file://{escaped}?mode=ro&immutable=1"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return false;
    };
    let version = connection.query_row("PRAGMA user_version", [], |row| row.get(0));
    let tables = connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table'
         AND name IN ('session', 'agent', 'message', 'read_mark', 'job')",
        [],
        |row| row.get::<_, i64>(0),
    );
    matches!((version, tables), (Ok(version), Ok(5)) if known_version(version))
}

fn migrate(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;

    let version: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 {
        // A new db has no schema. Tables at version 0 belong to another program (ADR 0036).
        let objects: i64 =
            tx.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))?;
        if objects > 0 {
            return Err("database not made by swarm; use another SWARM_HOME or move it".into());
        }
        tx.execute_batch(MIGRATIONS[0])?;
    } else if !known_version(version) {
        return Err(
            "database made by another swarm build; use another SWARM_HOME or delete it".into(),
        );
    }
    if version < 2 {
        tx.execute_batch(MIGRATIONS[1])?;
    }
    if version < 3 {
        tx.execute_batch(MIGRATIONS[2])?;
    }
    if version < 4 {
        tx.execute_batch(MIGRATIONS[3])?;
        tx.pragma_update(None, "user_version", 4)?;
    }
    if version < 5 {
        tx.execute_batch(MIGRATIONS[4])?;
        tx.pragma_update(None, "user_version", 5)?;
    }
    if version < 6 {
        tx.execute_batch(MIGRATIONS[5])?;
        tx.pragma_update(None, "user_version", 6)?;
    }

    tx.commit()?;

    Ok(())
}

pub fn enqueue_job(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    kind: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    connection.execute(
        "INSERT INTO job (session_id, agent_id, kind) VALUES (?1, ?2, ?3)",
        (session_id, agent_id, kind),
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn claim_next(connection: &Connection) -> Result<Option<i64>, Box<dyn std::error::Error>> {
    use rusqlite::OptionalExtension;
    let claimed = connection
        .query_row(
            "UPDATE job SET state = 'running', attempts = attempts + 1
             WHERE id = (SELECT id FROM job WHERE state = 'queued' AND run_after <= unixepoch()
                         ORDER BY id LIMIT 1)
             RETURNING id",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(claimed)
}

/// (session_id, agent_id, kind, attempts) of one job.
pub fn job(
    connection: &Connection,
    job_id: i64,
) -> Result<(String, String, String, i64), Box<dyn std::error::Error>> {
    let row = connection.query_row(
        "SELECT session_id, agent_id, kind, attempts FROM job WHERE id = ?1",
        [job_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(row)
}

pub fn finish_job(
    connection: &Connection,
    job_id: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE job SET state = 'done' WHERE id = ?1 AND state = 'running'",
        [job_id],
    )?;
    Ok(changed == 1)
}

pub fn release_job(
    connection: &Connection,
    job_id: i64,
    delay_secs: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE job SET state = 'queued', run_after = unixepoch() + ?2
         WHERE id = ?1 AND state = 'running'",
        [job_id, delay_secs],
    )?;
    Ok(changed == 1)
}

pub fn park_job(connection: &Connection, job_id: i64) -> Result<bool, Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE job SET state = 'parked' WHERE id = ?1 AND state = 'running'",
        [job_id],
    )?;
    Ok(changed == 1)
}

/// Writes `<path>.tmp` then renames it, so a reader never sees a partial file.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

pub fn send_message(
    connection: &mut Connection,
    root: &Path,
    session_id: &str,
    sender_id: &str,
    recipient_id: &str,
    kind: &str,
    body: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(seq), -1) + 1 FROM message WHERE session_id = ?1",
        [session_id],
        |row| row.get(0),
    )?;
    let mut body_path = format!("runs/{session_id}/{seq}.txt");
    let mut suffix = 0;
    while root.join(&body_path).exists() {
        suffix += 1;
        body_path = format!("runs/{session_id}/{seq}-{suffix}.txt");
    }
    tx.execute(
        "INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (session_id, seq, sender_id, recipient_id, kind, &body_path),
    )?;
    let file = root.join(&body_path);
    std::fs::create_dir_all(file.parent().ok_or("body path has no parent")?)?;
    write_atomic(&file, body)?;
    tx.commit()?;
    Ok(seq)
}

#[derive(Debug)]
pub struct Pending {
    pub seq: i64,
    pub sender_id: String,
    pub kind: String,
    pub body_path: String,
}

pub fn inbox(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<Vec<Pending>, Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE message SET seen_at = unixepoch()
         WHERE session_id = ?1 AND recipient_id = ?2 AND seen_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM read_mark
                           WHERE read_mark.session_id = message.session_id
                             AND message_seq = message.seq AND agent_id = ?2)",
        (session_id, agent_id),
    )?;
    let mut statement = connection.prepare(
        "SELECT seq, sender_id, kind, body_path FROM message
         WHERE session_id = ?1 AND recipient_id = ?2
           AND NOT EXISTS (SELECT 1 FROM read_mark
                           WHERE read_mark.session_id = message.session_id
                             AND message_seq = message.seq AND agent_id = ?2)
         ORDER BY seq",
    )?;
    let rows = statement.query_map((session_id, agent_id), |r| {
        Ok(Pending {
            seq: r.get(0)?,
            sender_id: r.get(1)?,
            kind: r.get(2)?,
            body_path: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn create_session(
    connection: &Connection,
    talk_mode: &str,
    cwd: &Path,
    chair: Option<(&str, &str)>,
    adapter: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let id = uuid::Uuid::now_v7().to_string();
    let cwd = cwd.to_string_lossy().into_owned();
    let (chair_provider, chair_id) = chair.unzip();
    connection.execute(
        "INSERT INTO session (id, talk_mode, cwd, adapter, chair_provider, chair_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (&id, talk_mode, cwd, adapter, chair_provider, chair_id),
    )?;
    Ok(id)
}

/// Link a newer session in the same directory; the link is written only after handoff delivery.
pub fn continue_session(
    connection: &Connection,
    new_id: &str,
    old_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE session SET continuation_of = ?2
         WHERE id = ?1 AND continuation_of IS NULL AND archived_at IS NULL
           AND EXISTS (SELECT 1 FROM session AS old
                       WHERE old.id = ?2 AND old.archived_at IS NULL
                         AND old.cwd = session.cwd AND old.id < session.id)",
        (new_id, old_id),
    )?;
    if changed != 1 {
        return Err("sessions cannot be linked as one chat".into());
    }
    Ok(())
}

pub fn set_chair(
    connection: &Connection,
    session_id: &str,
    chair: Option<(&str, &str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (provider, id) = chair.unzip();
    let changed = connection.execute(
        "UPDATE session SET chair_provider = ?2, chair_id = ?3, chair_log = NULL WHERE id = ?1",
        (session_id, provider, id),
    )?;
    if changed != 1 {
        return Err(format!("session {session_id} not found").into());
    }
    Ok(())
}

pub fn set_chair_log(
    connection: &Connection,
    session_id: &str,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE session SET chair_log = ?2 WHERE id = ?1",
        (session_id, path.to_string_lossy()),
    )?;
    Ok(())
}

pub fn set_adapter(
    connection: &Connection,
    session_id: &str,
    adapter: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE session SET adapter = ?2 WHERE id = ?1",
        (session_id, adapter),
    )?;
    if changed != 1 {
        return Err(format!("session {session_id} not found").into());
    }
    Ok(())
}

pub fn archive_sessions(
    connection: &mut Connection,
    session_ids: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;
    for session_id in session_ids {
        let changed = tx.execute(
            "UPDATE session SET archived_at = unixepoch() WHERE id = ?1",
            [session_id],
        )?;
        if changed != 1 {
            return Err(format!("swarm: no session {session_id}").into());
        }
    }
    tx.commit()?;
    Ok(())
}

#[derive(Debug)]
pub struct SessionRow {
    pub id: String,
    pub talk_mode: String,
    pub adapter: Option<String>,
    pub cwd: String,
    pub created_at: i64,
    pub chair_provider: Option<String>,
    pub chair_id: Option<String>,
    pub chair_log: Option<String>,
    pub continuation_of: Option<String>,
    pub chair_days: [String; 3],
    pub agents: i64,
    pub messages: i64,
    pub last_message_at: Option<i64>,
}

pub fn sessions(connection: &Connection) -> Result<Vec<SessionRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT session.id, talk_mode, adapter, cwd, session.created_at,
                chair_provider, chair_id, chair_log, continuation_of,
                strftime('%Y/%m/%d', session.created_at - 86400, 'unixepoch'),
                strftime('%Y/%m/%d', session.created_at, 'unixepoch'),
                strftime('%Y/%m/%d', session.created_at + 86400, 'unixepoch'),
                (SELECT count(*) FROM agent WHERE session_id = session.id),
                (SELECT count(*) FROM message WHERE session_id = session.id),
                (SELECT max(created_at) FROM message WHERE session_id = session.id)
         FROM session
         WHERE archived_at IS NULL
         ORDER BY session.created_at DESC, session.id DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(SessionRow {
            id: row.get(0)?,
            talk_mode: row.get(1)?,
            adapter: row.get(2)?,
            cwd: row.get(3)?,
            created_at: row.get(4)?,
            chair_provider: row.get(5)?,
            chair_id: row.get(6)?,
            chair_log: row.get(7)?,
            continuation_of: row.get(8)?,
            chair_days: [row.get(9)?, row.get(10)?, row.get(11)?],
            agents: row.get(12)?,
            messages: row.get(13)?,
            last_message_at: row.get(14)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

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

/// The CLI that runs in an agent's pane. A chair's row names none, so it is the session's
/// `chair_provider`.
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
    /// The chat log its provider's hooks reported; kept after the agent ends.
    pub log: Option<String>,
}

/// Record the chat log a provider hook reported for an agent.
pub fn set_log(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    log: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET log = ?3 WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id, log.to_string_lossy()),
    )?;
    Ok(())
}

/// Record an agent's reported state; `now` is unix seconds. The table's CHECKs refuse unknown
/// states and sources.
pub fn set_state(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    state: &str,
    source: &str,
    detail: Option<&str>,
    now: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET state = ?3, state_source = ?4, state_detail = ?5, state_at = ?6
         WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id, state, source, detail, now),
    )?;
    Ok(())
}

/// The report a listing read for one agent: (state, state_source, state_at).
pub type ReadState<'a> = (Option<&'a str>, Option<&'a str>, Option<i64>);

/// A screen check's write, applied only while the row still holds the report it read: a hook
/// that lands between the listing's read and this write keeps its newer report, also within the
/// same second, because state and source take part in the compare. Returns whether the row
/// changed.
pub fn set_screen_state(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    state: &str,
    detail: Option<&str>,
    now: i64,
    read: ReadState<'_>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (read_state, read_source, read_at) = read;
    let changed = connection.execute(
        "UPDATE agent SET state = ?3, state_source = 'screen', state_detail = ?4, state_at = ?5
         WHERE session_id = ?1 AND id = ?2
           AND state IS ?6 AND state_source IS ?7 AND state_at IS ?8",
        (
            session_id,
            agent_id,
            state,
            detail,
            now,
            read_state,
            read_source,
            read_at,
        ),
    )?;
    Ok(changed == 1)
}

pub fn agents(
    connection: &Connection,
    session_id: &str,
) -> Result<Vec<AgentRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT id, role, pane_id, provider, created_at, state, state_at, state_source, state_detail,
                log
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
            log: row.get(9)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Debug)]
pub struct MessageRow {
    pub seq: i64,
    pub sender: String,
    pub recipient: String,
    pub kind: String,
    pub body_path: String,
    pub created_at: i64,
    pub read: bool,
    pub delivery: Option<String>,
}

pub fn messages(
    connection: &Connection,
    session_id: &str,
    after: i64,
) -> Result<Vec<MessageRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT message.seq, sender_id, recipient_id, kind, body_path, created_at,
                EXISTS (SELECT 1 FROM read_mark
                        WHERE read_mark.session_id = message.session_id
                          AND message_seq = message.seq AND agent_id = message.recipient_id),
                delivery
         FROM message
         WHERE session_id = ?1 AND seq > ?2
         ORDER BY seq
         LIMIT 500",
    )?;
    let rows = statement.query_map((session_id, after), |row| {
        Ok(MessageRow {
            seq: row.get(0)?,
            sender: row.get(1)?,
            recipient: row.get(2)?,
            kind: row.get(3)?,
            body_path: row.get(4)?,
            created_at: row.get(5)?,
            read: row.get(6)?,
            delivery: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// What stalled a child (ADR 0041).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StallKind {
    Unacked,
    Silent,
}

impl StallKind {
    /// The name in the report kind `stall:<name>:<seq>` and in the sweep line.
    pub fn as_str(self) -> &'static str {
        match self {
            StallKind::Unacked => "unacked",
            StallKind::Silent => "silent",
        }
    }
}

/// A stalled child, what stalled it, and the message it is about.
pub type Stall = (String, StallKind, i64);

/// The stalls of a session's children that no report names yet, as (agent, what, seq) (ADR
/// 0041). `unacked`: the child is done, and the oldest message it read but did not ack was read
/// before it got done. `silent`: no such message, and the child got done after the last ring that
/// proved a turn, and sent nothing since that ring but its own reports. The chair is never the
/// subject: a message about the chair would go to the chair.
pub fn stalls(
    connection: &Connection,
    session_id: &str,
) -> Result<Vec<Stall>, Box<dyn std::error::Error>> {
    let children: Vec<(String, i64)> = connection
        .prepare(
            "SELECT id, state_at FROM agent
             WHERE session_id = ?1 AND pane_id IS NOT NULL AND state = 'done'
               AND state_at IS NOT NULL AND role != 'orchestrator' AND id != 'orchestrator'
             ORDER BY id",
        )?
        .query_map([session_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut found = Vec::new();
    for (agent, done_at) in children {
        let unacked: Option<(i64, i64)> = connection
            .query_row(
                "SELECT seq, seen_at FROM message
                 WHERE session_id = ?1 AND recipient_id = ?2 AND seen_at IS NOT NULL
                   AND NOT EXISTS (SELECT 1 FROM read_mark
                                   WHERE read_mark.session_id = message.session_id
                                     AND message_seq = message.seq AND agent_id = ?2)
                 ORDER BY seq LIMIT 1",
                (session_id, &agent),
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let stall = match unacked {
            Some((seq, seen_at)) if seen_at < done_at => Some((StallKind::Unacked, seq)),
            _ => connection
                .query_row(
                    "SELECT seq FROM message AS inbound
                     WHERE session_id = ?1 AND recipient_id = ?2
                       AND delivery IN ('hook', 'screen', 'seen') AND rung_at < ?3
                       AND NOT EXISTS (SELECT 1 FROM message
                                       WHERE session_id = ?1 AND sender_id = ?2
                                         AND created_at >= inbound.rung_at
                                         AND kind NOT GLOB 'stall:*'
                                         AND kind NOT GLOB 'unconfirmed:*')
                       AND seq = (SELECT MAX(seq) FROM message
                                  WHERE session_id = ?1 AND recipient_id = ?2
                                    AND delivery IN ('hook', 'screen', 'seen'))",
                    (session_id, &agent, done_at),
                    |row| row.get(0),
                )
                .optional()?
                .map(|seq| (StallKind::Silent, seq)),
        };
        let Some((what, seq)) = stall else {
            continue;
        };
        let reported: bool = connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM message
                            WHERE session_id = ?1 AND sender_id = ?2 AND kind = ?3)",
            (session_id, &agent, format!("stall:{}:{seq}", what.as_str())),
            |row| row.get(0),
        )?;
        if !reported {
            found.push((agent, what, seq));
        }
    }
    Ok(found)
}

/// A ring that no caller settled: (recipient, rung_at, the messages it rang, the lowest of them
/// that is unseen after `MAX_RINGS` rings).
pub type Ring = (String, i64, Vec<i64>, Option<i64>);

/// The rings with no result yet (ADR 0041): the listing's, which it types and leaves, and one whose
/// caller ended in its wait. One ring rings all its messages in the same second.
pub fn unsettled_rings(
    connection: &Connection,
    session_id: &str,
) -> Result<Vec<Ring>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT recipient_id, rung_at, seq, rings >= ?2 AND seen_at IS NULL FROM message
         WHERE session_id = ?1 AND rings > 0 AND rung_at IS NOT NULL AND delivery IS NULL
         ORDER BY recipient_id, rung_at, seq",
    )?;
    let rows = statement.query_map((session_id, MAX_RINGS), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, bool>(3)?,
        ))
    })?;
    let mut rings: Vec<Ring> = Vec::new();
    for row in rows {
        let (recipient, rung_at, seq, lost) = row?;
        match rings.last_mut() {
            Some(ring) if ring.0 == recipient && ring.1 == rung_at => {
                ring.2.push(seq);
                ring.3 = ring.3.or(lost.then_some(seq));
            }
            _ => rings.push((recipient, rung_at, vec![seq], lost.then_some(seq))),
        }
    }
    Ok(rings)
}

/// Messages whose last ring, after `MAX_RINGS` rings, proved nothing and that no
/// `unconfirmed:<seq>` report names yet, as (recipient, seq), the lowest seq of each ring. A
/// report that failed to send is found again. The chair is never the subject: a report about it
/// would go to it.
pub fn unreported_lost(
    connection: &Connection,
    session_id: &str,
    chair: &str,
) -> Result<Vec<(String, i64)>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT recipient_id, seq FROM (
             SELECT recipient_id, MIN(seq) AS seq FROM message
             WHERE session_id = ?1 AND recipient_id != ?2 AND rings >= ?3
               AND delivery = 'unconfirmed' AND seen_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM read_mark
                               WHERE read_mark.session_id = message.session_id
                                 AND message_seq = message.seq AND agent_id = recipient_id)
             GROUP BY recipient_id, rung_at
         ) AS lost
         WHERE NOT EXISTS (SELECT 1 FROM message
                           WHERE session_id = ?1 AND sender_id = lost.recipient_id
                             AND kind = 'unconfirmed:' || lost.seq)
         ORDER BY recipient_id, seq",
    )?;
    let rows = statement.query_map((session_id, chair, MAX_RINGS), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Whether a hook of `agent_id` reported a turn at or after `since` (unix seconds). `done` is no
/// turn start, because Claude's idle notice writes it on an idle pane.
pub fn turn_started(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    since: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let started: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM agent
                        WHERE session_id = ?1 AND id = ?2 AND state_source = 'hook'
                          AND state IN ('working', 'waiting') AND state_at >= ?3)",
        (session_id, agent_id, since),
        |row| row.get(0),
    )?;
    Ok(started)
}

/// Whether `agent_id` read its messages at or after `since` (unix seconds). `inbox` marks every
/// unseen message of the agent at once, so a read after a ring also read the messages it rang.
pub fn seen_since(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    since: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let seen: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM message
                        WHERE session_id = ?1 AND recipient_id = ?2 AND seen_at >= ?3)",
        (session_id, agent_id, since),
        |row| row.get(0),
    )?;
    Ok(seen)
}

/// Store what a ring proved on the messages it rang at `rung_at`. The column's CHECK refuses an
/// unknown value. A message rung again since, or with a result already, keeps what it has, so a
/// pass that read an older ring cannot settle a newer one. One statement writes all the messages,
/// so of two passes that settle one ring, one stores the result on all of them and the other on
/// none. Returns whether this call stored it, so the pass that lost the race reports nothing.
pub fn set_delivery(
    connection: &Connection,
    session_id: &str,
    seqs: &[i64],
    rung_at: i64,
    delivery: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let stored = connection.execute(
        "UPDATE message SET delivery = ?3
         WHERE session_id = ?1 AND seq IN (SELECT value FROM json_each(?2)) AND rung_at = ?4
           AND delivery IS NULL",
        (session_id, serde_json::to_string(seqs)?, delivery, rung_at),
    )?;
    Ok(stored > 0)
}

pub fn has_rung_unread(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let found: bool = connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM message
             WHERE session_id = ?1 AND recipient_id = ?2 AND rings > 0
               AND seen_at IS NULL
               AND NOT EXISTS (
                   SELECT 1 FROM read_mark
                   WHERE read_mark.session_id = message.session_id
                     AND message_seq = message.seq AND agent_id = ?2
               )
         )",
        (session_id, agent_id),
        |row| row.get(0),
    )?;
    Ok(found)
}

pub fn has_unrung_unread(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let found: bool = connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM message
             WHERE session_id = ?1 AND recipient_id = ?2 AND rings = 0
               AND NOT EXISTS (
                   SELECT 1 FROM read_mark
                   WHERE read_mark.session_id = message.session_id
                     AND message_seq = message.seq AND agent_id = ?2
               )
         )",
        (session_id, agent_id),
        |row| row.get(0),
    )?;
    Ok(found)
}

/// How many rings a message gets in all. After the last one proves nothing, the chair is told
/// (ADR 0041).
pub const MAX_RINGS: i64 = 2;

pub fn rering_due(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    age_secs: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let due: bool = connection.query_row(
        "SELECT COUNT(*) > 0
             AND MIN(created_at) <= unixepoch() - ?3
             AND (MAX(rung_at) IS NULL OR MAX(rung_at) <= unixepoch() - ?3)
             AND MAX(rings) < ?4
         FROM message
         WHERE session_id = ?1 AND recipient_id = ?2
           AND seen_at IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM read_mark
               WHERE read_mark.session_id = message.session_id
                 AND message_seq = message.seq AND agent_id = ?2
           )",
        (session_id, agent_id, age_secs, MAX_RINGS),
        |row| row.get(0),
    )?;
    Ok(due)
}

/// Mark `agent_id`'s unseen messages as rung once more, and return each one's seq and ring
/// count. The result goes back to NULL, because this ring has none yet. A message rung within
/// `age_secs` of `rung_at`, or at its last ring, is left out, so a second pass that found the same
/// re-ring due rings nothing.
pub fn rering(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    age_secs: i64,
    rung_at: i64,
) -> Result<Vec<(i64, i64)>, Box<dyn std::error::Error>> {
    let rung = connection
        .prepare(
            "UPDATE message SET rung_at = ?5, rings = rings + 1, delivery = NULL
             WHERE session_id = ?1 AND recipient_id = ?2 AND seen_at IS NULL
               AND (rung_at IS NULL OR rung_at <= ?5 - ?3) AND rings < ?4
               AND NOT EXISTS (SELECT 1 FROM read_mark
                               WHERE read_mark.session_id = message.session_id
                                 AND message_seq = message.seq AND agent_id = ?2)
             RETURNING seq, rings",
        )?
        .query_map(
            (session_id, agent_id, age_secs, MAX_RINGS, rung_at),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .collect::<Result<_, _>>()?;
    Ok(rung)
}

pub fn clear_pane(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        // A closed agent reports nothing, so it shows as ended rather than its last state.
        "UPDATE agent SET pane_id = NULL, state = NULL, state_at = NULL, state_source = NULL,
         state_detail = NULL WHERE session_id = ?1 AND id = ?2",
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

pub fn has_summary(
    connection: &Connection,
    session_id: &str,
    sender_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM message WHERE session_id = ?1 AND sender_id = ?2 AND kind = 'summary'",
        (session_id, sender_id),
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Apply the session talk mode: open passes, lane blocks child-to-child, relay sends
/// child-to-child to the orchestrator as `relay:<recipient>`. Returns (recipient, kind).
pub fn route(
    connection: &Connection,
    session_id: &str,
    sender: &str,
    recipient: &str,
    kind: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let mode: String = connection.query_row(
        "SELECT talk_mode FROM session WHERE id = ?1",
        [session_id],
        |r| r.get(0),
    )?;
    let orchestrator = orchestrator_of(connection, session_id)?;
    if mode == "open" || sender == orchestrator || recipient == orchestrator {
        return Ok((recipient.to_string(), kind.to_string()));
    }
    if mode == "lane" {
        return Err(format!("lane: {sender} cannot message {recipient}").into());
    }
    Ok((orchestrator, format!("relay:{recipient}")))
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

pub fn ack(
    connection: &Connection,
    session_id: &str,
    seq: i64,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "INSERT INTO read_mark (session_id, message_seq, agent_id)
         SELECT session_id, seq, recipient_id FROM message
         WHERE session_id = ?1 AND seq = ?2 AND recipient_id = ?3
         ON CONFLICT (session_id, message_seq, agent_id) DO NOTHING",
        (session_id, seq, agent_id),
    )?;
    if changed == 0
        && !connection.query_row(
            "SELECT EXISTS (
             SELECT 1 FROM read_mark
             JOIN message ON message.session_id = read_mark.session_id
                         AND message.seq = read_mark.message_seq
             WHERE message.session_id = ?1
               AND read_mark.message_seq = ?2
               AND read_mark.agent_id = ?3
         )",
            (session_id, seq, agent_id),
            |row| row.get(0),
        )?
    {
        return Err(
            format!("message {seq} is not for agent {agent_id} in session {session_id}").into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = "0199a000-0000-7000-8000-000000000001";
    const OTHER_SESSION: &str = "0199a000-0000-7000-8000-000000000002";
    const ORCHESTRATOR: &str = "orchestrator";
    const CODER: &str = "coder";
    const OUTSIDER: &str = "outsider";

    fn seed(run_after: i64) -> Connection {
        let connection = open(Path::new(":memory:")).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test'), ('{OTHER_SESSION}', 'lane', '/other');
                 INSERT INTO agent (id, session_id, role) VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'), ('{CODER}', '{SESSION}', 'coder'), ('{OUTSIDER}', '{OTHER_SESSION}', 'coder');
                 INSERT INTO job (id, session_id, agent_id, kind, run_after) VALUES (7, '{SESSION}', '{CODER}', 'build', {run_after});"
            ))
            .unwrap();
        connection
    }

    fn unix_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("swarm-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn drain(db: std::path::PathBuf) -> std::thread::JoinHandle<Vec<i64>> {
        std::thread::spawn(move || {
            let connection = open(&db).unwrap();
            let mut claimed = Vec::new();
            while let Some(id) = claim_next(&connection).unwrap() {
                assert!(finish_job(&connection, id).unwrap());
                claimed.push(id);
            }
            claimed
        })
    }

    fn state_and_attempts(connection: &Connection) -> (String, i64) {
        connection
            .query_row("SELECT state, attempts FROM job WHERE id = 7", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap()
    }

    #[test]
    fn claims_due_queued_job() {
        let connection = seed(0);
        assert_eq!(claim_next(&connection).unwrap(), Some(7));
        assert_eq!(
            job(&connection, 7).unwrap(),
            (
                SESSION.to_string(),
                CODER.to_string(),
                "build".to_string(),
                1
            )
        );
        assert_eq!(state_and_attempts(&connection), ("running".into(), 1));
    }

    #[test]
    fn skips_claimed_and_future_jobs() {
        let connection = seed(0);
        assert_eq!(claim_next(&connection).unwrap(), Some(7));
        assert_eq!(claim_next(&connection).unwrap(), None);
        assert_eq!(state_and_attempts(&connection), ("running".into(), 1));

        let future = seed(i64::MAX);
        assert_eq!(claim_next(&future).unwrap(), None);
        assert_eq!(state_and_attempts(&future), ("queued".into(), 0));
    }

    #[test]
    fn returns_error_without_job_table() {
        let bare = Connection::open_in_memory().unwrap();
        assert!(claim_next(&bare).is_err());
    }

    #[test]
    fn finishes_running_job_once() {
        let connection = seed(0);
        assert!(!finish_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 0));

        assert_eq!(claim_next(&connection).unwrap(), Some(7));
        assert!(finish_job(&connection, 7).unwrap());
        assert!(!finish_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("done".into(), 1));
    }

    #[test]
    fn releases_running_job_with_delay() {
        let connection = seed(0);
        assert!(!release_job(&connection, 7, 60).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 0));

        assert_eq!(claim_next(&connection).unwrap(), Some(7));
        assert!(release_job(&connection, 7, 60).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 1));
        assert_eq!(claim_next(&connection).unwrap(), None);

        let retry = seed(0);
        assert_eq!(claim_next(&retry).unwrap(), Some(7));
        assert!(release_job(&retry, 7, 0).unwrap());
        assert_eq!(claim_next(&retry).unwrap(), Some(7));
        assert_eq!(state_and_attempts(&retry), ("running".into(), 2));
    }

    #[test]
    fn parks_running_job_for_good() {
        let connection = seed(0);
        assert!(!park_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 0));

        assert_eq!(claim_next(&connection).unwrap(), Some(7));
        assert!(park_job(&connection, 7).unwrap());
        assert_eq!(claim_next(&connection).unwrap(), None);
        assert!(!release_job(&connection, 7, 0).unwrap());
        assert!(!park_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("parked".into(), 1));
    }

    #[test]
    fn sends_message_row_and_file() {
        let mut connection = seed(0);
        let root = temp_root("send");
        let seq = send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "note",
            "hello",
        )
        .unwrap();
        assert_eq!(seq, 0);
        let body_path: String = connection
            .query_row("SELECT body_path FROM message WHERE seq = 0", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(body_path, format!("runs/{SESSION}/0.txt"));
        assert_eq!(
            std::fs::read_to_string(root.join(body_path)).unwrap(),
            "hello"
        );
        assert!(!root.join(format!("runs/{SESSION}/0.tmp")).exists());

        assert!(
            send_message(
                &mut connection,
                &root,
                SESSION,
                ORCHESTRATOR,
                OUTSIDER,
                "note",
                "x"
            )
            .is_err()
        );
        let count: i64 = connection
            .query_row("SELECT count(*) FROM message", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!root.join(format!("runs/{SESSION}/1.txt")).exists());
    }

    #[test]
    fn inbox_lists_pending_in_seq_order_and_stamps_seen_once() {
        let mut connection = seed(0);
        let root = temp_root("inbox");
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "note",
            "one",
        )
        .unwrap();
        send_message(
            &mut connection,
            &root,
            SESSION,
            CODER,
            ORCHESTRATOR,
            "note",
            "reply",
        )
        .unwrap();
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "two",
        )
        .unwrap();

        let pending = inbox(&connection, SESSION, CODER).unwrap();
        let seen: Vec<(i64, &str, &str, &str)> = pending
            .iter()
            .map(|m| {
                (
                    m.seq,
                    m.sender_id.as_str(),
                    m.kind.as_str(),
                    m.body_path.as_str(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                (
                    0,
                    ORCHESTRATOR,
                    "note",
                    format!("runs/{SESSION}/0.txt").as_str()
                ),
                (
                    2,
                    ORCHESTRATOR,
                    "ask",
                    format!("runs/{SESSION}/2.txt").as_str()
                )
            ]
        );
        let seen_at: i64 = connection
            .query_row("SELECT seen_at FROM message WHERE seq = 0", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(seen_at > 0);
        connection
            .execute("UPDATE message SET seen_at = 7 WHERE seq = 0", [])
            .unwrap();
        inbox(&connection, SESSION, CODER).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT seen_at FROM message WHERE seq = 0", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(inbox(&connection, SESSION, ORCHESTRATOR).unwrap().len(), 1);
        assert!(
            inbox(&connection, OTHER_SESSION, OUTSIDER)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn ack_marks_once_and_only_for_recipient() {
        let mut connection = seed(0);
        let root = temp_root("ack");
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "note",
            "one",
        )
        .unwrap();
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "note",
            "two",
        )
        .unwrap();

        assert!(ack(&connection, SESSION, 0, ORCHESTRATOR).is_err());
        assert!(ack(&connection, OTHER_SESSION, 0, CODER).is_err());
        assert_eq!(inbox(&connection, SESSION, CODER).unwrap().len(), 2);

        ack(&connection, SESSION, 0, CODER).unwrap();
        ack(&connection, SESSION, 0, CODER).unwrap();
        let marks: i64 = connection
            .query_row("SELECT count(*) FROM read_mark", [], |r| r.get(0))
            .unwrap();
        assert_eq!(marks, 1);
        assert_eq!(inbox(&connection, SESSION, CODER).unwrap()[0].seq, 1);
    }

    #[test]
    fn creates_sessions_with_unique_uuid_v7_ids() {
        let connection = seed(0);
        let relay = create_session(&connection, "relay", Path::new("/relay"), None, None).unwrap();
        let open_id = create_session(&connection, "open", Path::new("/open"), None, None).unwrap();
        assert_eq!(uuid::Uuid::parse_str(&relay).unwrap().get_version_num(), 7);
        assert_ne!(relay, open_id);
        assert!(create_session(&connection, "loud", Path::new("/loud"), None, None).is_err());
        let count: i64 = connection
            .query_row("SELECT count(*) FROM session", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn agent_ids_are_unique_per_session() {
        let connection = open(Path::new(":memory:")).unwrap();
        let first = create_session(&connection, "lane", Path::new("/first"), None, None).unwrap();
        let second = create_session(&connection, "lane", Path::new("/second"), None, None).unwrap();
        add_agent(&connection, &first, ORCHESTRATOR, "orchestrator").unwrap();
        add_agent(&connection, &second, ORCHESTRATOR, "orchestrator").unwrap();
        set_pane(&connection, &first, ORCHESTRATOR, "%1").unwrap();
        set_pane(&connection, &second, ORCHESTRATOR, "%2").unwrap();
        set_provider(&connection, &second, ORCHESTRATOR, "codex").unwrap();
        let second_agents = agents(&connection, &second).unwrap();
        let second_agent = &second_agents[0];
        assert_eq!(second_agent.provider.as_deref(), Some("codex"));
        assert!(second_agent.created_at > 0);
        assert!(add_agent(&connection, &second, ORCHESTRATOR, "orchestrator").is_err());
        assert!(add_agent(&connection, "missing", "ghost", "coder").is_err());
        assert_eq!(
            pane_of(&connection, &first, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%1")
        );
        assert_eq!(
            pane_of(&connection, &second, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%2")
        );
    }

    #[test]
    fn a_chair_pane_finds_its_newest_session_and_no_archived_one() {
        let mut connection = open(Path::new(":memory:")).unwrap();
        let older = create_session(
            &connection,
            "lane",
            Path::new("/older"),
            None,
            Some("herdr"),
        )
        .unwrap();
        let newer = create_session(
            &connection,
            "lane",
            Path::new("/newer"),
            None,
            Some("herdr"),
        )
        .unwrap();
        for session in [&older, &newer] {
            add_agent(&connection, session, ORCHESTRATOR, "orchestrator").unwrap();
        }
        add_agent(&connection, &older, "worker", "code.complex").unwrap();
        set_pane(&connection, &older, "worker", "wK:p2").unwrap();
        set_pane(&connection, &older, ORCHESTRATOR, "wK:p1").unwrap();
        assert_eq!(
            orchestrator_at_pane(&connection, "wK:p1").unwrap(),
            Some((older.clone(), ORCHESTRATOR.to_string()))
        );
        assert_eq!(orchestrator_at_pane(&connection, "wK:p2").unwrap(), None);

        set_pane(&connection, &newer, ORCHESTRATOR, "wK:p1").unwrap();
        assert_eq!(
            orchestrator_at_pane(&connection, "wK:p1").unwrap(),
            Some((newer.clone(), ORCHESTRATOR.to_string()))
        );
        archive_sessions(&mut connection, &[newer]).unwrap();
        assert_eq!(orchestrator_at_pane(&connection, "wK:p1").unwrap(), None);
    }

    #[test]
    fn an_orchestrator_pane_moves_only_within_one_adapter() {
        let connection = open(Path::new(":memory:")).unwrap();
        let tmux =
            create_session(&connection, "lane", Path::new("/tmux"), None, Some("tmux")).unwrap();
        let same =
            create_session(&connection, "lane", Path::new("/same"), None, Some("tmux")).unwrap();
        let solo = create_session(
            &connection,
            "lane",
            Path::new("/solo"),
            None,
            Some("tmux-solo"),
        )
        .unwrap();
        for session in [&tmux, &same, &solo] {
            add_agent(&connection, session, ORCHESTRATOR, "code.complex").unwrap();
        }

        set_pane(&connection, &tmux, ORCHESTRATOR, "%0").unwrap();
        set_pane(&connection, &solo, ORCHESTRATOR, "%0").unwrap();
        assert_eq!(
            pane_of(&connection, &tmux, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );
        assert_eq!(
            pane_of(&connection, &solo, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );

        set_pane(&connection, &same, ORCHESTRATOR, "%0").unwrap();
        assert_eq!(pane_of(&connection, &tmux, ORCHESTRATOR).unwrap(), None);
        assert_eq!(
            pane_of(&connection, &same, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );
        assert_eq!(
            pane_of(&connection, &solo, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%0")
        );
    }

    /// A stale adapter answered `self` with nothing. The empty answer was stored, and the chat
    /// said `no pane recorded` at the first message rather than at registration.
    #[test]
    fn an_empty_pane_is_refused() {
        let connection = seed(0);
        let refused = set_pane(&connection, SESSION, ORCHESTRATOR, "  ")
            .unwrap_err()
            .to_string();
        assert_eq!(refused, "swarm: adapter gave no pane for orchestrator");
        assert_eq!(pane_of(&connection, SESSION, ORCHESTRATOR).unwrap(), None);
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(
            pane_of(&connection, SESSION, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%1")
        );
    }

    #[test]
    fn two_drainers_claim_every_job_once() {
        let db = temp_root("drain").join("swarm.db");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let connection = open(&db).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role) VALUES ('{CODER}', '{SESSION}', 'coder');"
            ))
            .unwrap();
        for _ in 0..200 {
            enqueue_job(&connection, SESSION, CODER, "summarize").unwrap();
        }
        let (left, right) = (drain(db.clone()), drain(db.clone()));
        let mut all = left.join().unwrap();
        all.extend(right.join().unwrap());
        all.sort();
        assert_eq!(all, (1..=200).collect::<Vec<i64>>());
        let (done, once): (i64, i64) = connection
            .query_row(
                "SELECT sum(state = 'done'), sum(attempts = 1) FROM job",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((done, once), (200, 200));
    }

    #[test]
    fn sets_and_reads_pane() {
        let connection = seed(0);
        assert_eq!(pane_of(&connection, SESSION, CODER).unwrap(), None);
        set_pane(&connection, SESSION, CODER, "w8A:p2").unwrap();
        assert_eq!(
            pane_of(&connection, SESSION, CODER).unwrap().as_deref(),
            Some("w8A:p2")
        );
        assert!(pane_of(&connection, OTHER_SESSION, CODER).is_err());
        assert!(pane_of(&connection, SESSION, "ghost").is_err());
    }

    #[test]
    fn rings_only_unread_and_rerings_once_until_seen() {
        let mut connection = seed(0);
        let root = temp_root("old-unread");
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "old",
        )
        .unwrap();
        send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "new",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61 WHERE seq = 0",
                [],
            )
            .unwrap();

        assert!(!has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 62).unwrap());
        assert!(!rering_due(&connection, OTHER_SESSION, OUTSIDER, 60).unwrap());
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch(), rings = 1 WHERE seq = 0",
                [],
            )
            .unwrap();
        assert!(has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - 61 WHERE seq = 0",
                [],
            )
            .unwrap();
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection
            .execute("UPDATE message SET rings = 2 WHERE seq = 0", [])
            .unwrap();
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        inbox(&connection, SESSION, CODER).unwrap();
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        ack(&connection, SESSION, 0, CODER).unwrap();
        assert!(!has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
    }

    /// A sweep and a listing can both find a re-ring due before either one rings. The second
    /// update finds the messages rung a moment ago, so it rings nothing and the cap holds.
    #[test]
    fn two_passes_that_find_a_rering_due_ring_it_once() {
        let mut connection = seed(0);
        let root = temp_root("rering-race");
        let ask = send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1, delivery = 'unconfirmed'",
                [],
            )
            .unwrap();
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());

        let now = unix_now();
        assert_eq!(
            rering(&connection, SESSION, CODER, 60, now).unwrap(),
            [(ask, 2)]
        );
        assert_eq!(rering(&connection, SESSION, CODER, 60, now).unwrap(), []);
        assert_eq!(
            messages(&connection, SESSION, -1).unwrap()[0].delivery,
            None
        );
    }

    /// A pass that settles a ring stores its result only on that ring, once. A ring that another
    /// pass rang again since, or that has a result already, keeps what it has.
    #[test]
    fn a_ring_result_is_stored_only_on_the_ring_it_settles() {
        let mut connection = seed(0);
        let root = temp_root("delivery-race");
        let ask = send_message(
            &mut connection,
            &root,
            SESSION,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        let first_ring = unix_now() - 61;
        connection
            .execute(
                "UPDATE message SET created_at = ?1, rung_at = ?1, rings = 1",
                [first_ring],
            )
            .unwrap();
        let delivery = |connection: &Connection| {
            messages(connection, SESSION, -1).unwrap()[0]
                .delivery
                .clone()
        };

        let second_ring = unix_now();
        rering(&connection, SESSION, CODER, 60, second_ring).unwrap();
        assert!(!set_delivery(&connection, SESSION, &[ask], first_ring, "unconfirmed").unwrap());
        assert_eq!(delivery(&connection), None);
        assert!(set_delivery(&connection, SESSION, &[ask], second_ring, "screen").unwrap());
        assert!(!set_delivery(&connection, SESSION, &[ask], second_ring, "unconfirmed").unwrap());
        assert_eq!(delivery(&connection).as_deref(), Some("screen"));
    }

    /// One ring's result is one write over every message it rang. Of two passes that settle a
    /// ring at the same time, exactly one stores the result and reports it, and the ring keeps
    /// one result.
    #[test]
    fn two_passes_settle_a_ring_of_many_messages_once() {
        let db = temp_root("settle-ring").join("swarm.db");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let connection = open(&db).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role)
                     VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'),
                            ('{CODER}', '{SESSION}', 'coder');
                 WITH RECURSIVE ring(seq) AS (SELECT 0 UNION ALL SELECT seq + 1 FROM ring
                                              WHERE seq < 499)
                 INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path,
                                      rung_at, rings)
                     SELECT '{SESSION}', seq, '{ORCHESTRATOR}', '{CODER}', 'ask',
                            'runs/' || seq || '.txt', 1700, 1
                     FROM ring;"
            ))
            .unwrap();
        // The race is timing, so it runs several times.
        for _ in 0..10 {
            connection
                .execute("UPDATE message SET delivery = NULL", [])
                .unwrap();
            let start = std::sync::Arc::new(std::sync::Barrier::new(2));
            let passes = ["hook", "unconfirmed"].map(|delivery| {
                let (db, start) = (db.clone(), start.clone());
                std::thread::spawn(move || {
                    let connection = open(&db).unwrap();
                    let seqs: Vec<i64> = (0..500).collect();
                    start.wait();
                    set_delivery(&connection, SESSION, &seqs, 1700, delivery).unwrap()
                })
            });
            let stored = passes.map(|pass| pass.join().unwrap());
            assert_eq!(stored.iter().filter(|stored| **stored).count(), 1);
            let results: Vec<Option<String>> = connection
                .prepare("SELECT DISTINCT delivery FROM message")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(results.len(), 1, "{results:?}");
            assert!(results[0].is_some());
        }
    }

    /// A ring's result is stored on the messages it rang, and a stall or lost-ring report is
    /// stored once per sender and kind (ADR 0041).
    #[test]
    fn stores_a_ring_result_and_each_report_once() {
        let mut connection = seed(0);
        let root = temp_root("delivery");
        let send = |connection: &mut Connection, kind: &str| {
            send_message(
                connection,
                &root,
                SESSION,
                CODER,
                ORCHESTRATOR,
                kind,
                "body",
            )
        };
        let ask = send(&mut connection, "ask").unwrap();
        let rung_at = unix_now();
        connection
            .execute("UPDATE message SET rung_at = ?1, rings = 1", [rung_at])
            .unwrap();
        assert_eq!(
            messages(&connection, SESSION, -1).unwrap()[0].delivery,
            None
        );
        assert!(set_delivery(&connection, SESSION, &[ask], rung_at, "maybe").is_err());
        set_delivery(&connection, SESSION, &[ask], rung_at, "hook").unwrap();
        assert_eq!(
            messages(&connection, SESSION, -1).unwrap()[0]
                .delivery
                .as_deref(),
            Some("hook")
        );
        send(&mut connection, "stall:unacked:0").unwrap();
        assert!(send(&mut connection, "stall:unacked:0").is_err());
        send(&mut connection, "unconfirmed:0").unwrap();
        assert!(send(&mut connection, "unconfirmed:0").is_err());
        send(&mut connection, "note").unwrap();
        send(&mut connection, "note").unwrap();
    }

    /// A child at done with a message it read and did not ack is stalled; so is a child at done
    /// that sent nothing after a ring that started its turn. A reported stall is not found again,
    /// and the chair is never the subject (ADR 0041).
    #[test]
    fn finds_each_stall_once() {
        let mut connection = seed(0);
        let root = temp_root("stalls");
        let run = |connection: &Connection, sql: &str| connection.execute_batch(sql).unwrap();
        let send = |connection: &mut Connection, from: &str, to: &str, kind: &str| {
            send_message(connection, &root, SESSION, from, to, kind, "body").unwrap()
        };
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        let ask = send(&mut connection, ORCHESTRATOR, CODER, "ask");
        run(
            &connection,
            "UPDATE message SET rung_at = unixepoch() - 30, rings = 1, delivery = 'hook'",
        );
        // Working, or done with no read message and a reply sent: no stall.
        set_state(&connection, SESSION, CODER, "working", "hook", None, 0).unwrap();
        assert!(stalls(&connection, SESSION).unwrap().is_empty());

        // Read, then done without an ack.
        run(&connection, "UPDATE message SET seen_at = unixepoch() - 20");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        set_state(&connection, SESSION, CODER, "done", "hook", None, now).unwrap();
        assert_eq!(
            stalls(&connection, SESSION).unwrap(),
            [(CODER.to_string(), StallKind::Unacked, ask)]
        );
        send(
            &mut connection,
            CODER,
            ORCHESTRATOR,
            &format!("stall:unacked:{ask}"),
        );
        assert!(stalls(&connection, SESSION).unwrap().is_empty());

        // Acked, and its own report is no reply: it finished silent.
        ack(&connection, SESSION, ask, CODER).unwrap();
        assert_eq!(
            stalls(&connection, SESSION).unwrap(),
            [(CODER.to_string(), StallKind::Silent, ask)]
        );
        send(
            &mut connection,
            CODER,
            ORCHESTRATOR,
            &format!("stall:silent:{ask}"),
        );
        assert!(stalls(&connection, SESSION).unwrap().is_empty());

        // A reply after the ring is no stall; nor is the chair at done with unacked work.
        run(&connection, "DELETE FROM message WHERE kind GLOB 'stall:*'");
        send(&mut connection, CODER, ORCHESTRATOR, "summary");
        run(
            &connection,
            "UPDATE message SET seen_at = unixepoch() - 20 WHERE sender_id = 'coder'",
        );
        set_state(
            &connection,
            SESSION,
            ORCHESTRATOR,
            "done",
            "hook",
            None,
            now,
        )
        .unwrap();
        assert!(stalls(&connection, SESSION).unwrap().is_empty());
    }

    #[test]
    fn finds_the_session_orchestrator() {
        let connection = seed(0);
        assert_eq!(orchestrator_of(&connection, SESSION).unwrap(), ORCHESTRATOR);
        connection
            .execute(
                "UPDATE agent SET role = 'code.complex' WHERE session_id = ?1 AND id = ?2",
                (SESSION, ORCHESTRATOR),
            )
            .unwrap();
        assert_eq!(orchestrator_of(&connection, SESSION).unwrap(), ORCHESTRATOR);
        assert_eq!(
            orchestrator_of(&connection, OTHER_SESSION)
                .unwrap_err()
                .to_string(),
            format!("session {OTHER_SESSION} has no orchestrator")
        );
    }

    #[test]
    fn lists_live_children_and_summaries() {
        let mut connection = seed(0);
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(
            live_children(&connection, SESSION, ORCHESTRATOR).unwrap(),
            [(CODER.to_string(), "%2".to_string())]
        );
        clear_pane(&connection, SESSION, CODER).unwrap();
        assert!(
            live_children(&connection, SESSION, ORCHESTRATOR)
                .unwrap()
                .is_empty()
        );
        assert!(!has_summary(&connection, SESSION, CODER).unwrap());
        send_message(
            &mut connection,
            &temp_root("summary"),
            SESSION,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        assert!(has_summary(&connection, SESSION, CODER).unwrap());
    }

    #[test]
    fn routes_by_talk_mode() {
        let connection = seed(0);
        connection.execute_batch(&format!("INSERT INTO agent (id, session_id, role) VALUES ('reviewer', '{SESSION}', 'reviewer')")).unwrap();
        let set_mode = |mode: &str| {
            connection
                .execute(
                    "UPDATE session SET talk_mode = ?1 WHERE id = ?2",
                    (mode, SESSION),
                )
                .unwrap()
        };
        assert_eq!(
            route(&connection, SESSION, CODER, "reviewer", "ask")
                .unwrap_err()
                .to_string(),
            "lane: coder cannot message reviewer"
        );
        assert_eq!(
            route(&connection, SESSION, CODER, ORCHESTRATOR, "ask").unwrap(),
            (ORCHESTRATOR.into(), "ask".into())
        );
        set_mode("relay");
        assert_eq!(
            route(&connection, SESSION, CODER, "reviewer", "ask").unwrap(),
            (ORCHESTRATOR.into(), "relay:reviewer".into())
        );
        set_mode("open");
        assert_eq!(
            route(&connection, SESSION, CODER, "reviewer", "ask").unwrap(),
            ("reviewer".into(), "ask".into())
        );
    }

    #[test]
    fn session_ids_are_never_reused() {
        let connection = open(Path::new(":memory:")).unwrap();
        let first = create_session(&connection, "lane", Path::new("/first"), None, None).unwrap();
        connection
            .execute("DELETE FROM session WHERE id = ?1", [&first])
            .unwrap();
        assert_ne!(
            create_session(&connection, "lane", Path::new("/second"), None, None).unwrap(),
            first
        );
    }

    #[test]
    fn migration_adds_chat_continuations_to_existing_databases() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        migrate(&mut connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 6);
        let old = create_session(&connection, "lane", Path::new("/work"), None, None).unwrap();
        let new = create_session(&connection, "lane", Path::new("/work"), None, None).unwrap();
        continue_session(&connection, &new, &old).unwrap();
        assert_eq!(
            sessions(&connection).unwrap()[0].continuation_of.as_deref(),
            Some(old.as_str())
        );
        assert!(continue_session(&connection, &old, &new).is_err());
    }

    #[test]
    fn migration_adds_agent_state_to_a_version_two_database() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection.execute_batch(MIGRATIONS[1]).unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role) VALUES ('{CODER}', '{SESSION}', 'coder');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 6);
        let coder = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(coder.state, None);
        set_state(&connection, SESSION, CODER, "waiting", "hook", None, 1_700).unwrap();
        let coder = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(coder.state.as_deref(), Some("waiting"));
        assert_eq!(coder.state_at, Some(1_700));
        assert_eq!(coder.state_source.as_deref(), Some("hook"));
    }

    #[test]
    fn sets_state_only_for_one_agent_and_refuses_unknown_values() {
        let connection = seed(0);
        set_state(
            &connection,
            SESSION,
            CODER,
            "failed",
            "hook",
            Some("rate_limit"),
            42,
        )
        .unwrap();
        let rows = agents(&connection, SESSION).unwrap();
        let coder = rows.iter().find(|row| row.id == CODER).unwrap();
        let orchestrator = rows.iter().find(|row| row.id == ORCHESTRATOR).unwrap();
        assert_eq!(coder.state.as_deref(), Some("failed"));
        assert_eq!(coder.state_detail.as_deref(), Some("rate_limit"));
        assert_eq!(orchestrator.state, None);
        assert!(set_state(&connection, SESSION, CODER, "asleep", "hook", None, 42).is_err());
        assert!(set_state(&connection, SESSION, CODER, "done", "guess", None, 42).is_err());
    }

    #[test]
    fn a_screen_write_does_not_overwrite_a_newer_hook_report() {
        let connection = seed(0);
        set_state(&connection, SESSION, CODER, "working", "hook", None, 100).unwrap();
        // The listing read state_at 100; a hook reports waiting before the screen write lands.
        set_state(&connection, SESSION, CODER, "waiting", "hook", None, 105).unwrap();
        let read = (Some("working"), Some("hook"), Some(100));
        let written =
            set_screen_state(&connection, SESSION, CODER, "done", None, 110, read).unwrap();
        assert!(!written);
        let coder = agents(&connection, SESSION)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.state.as_deref(), Some("waiting"));
        assert_eq!(coder.state_source.as_deref(), Some("hook"));
        let latest = (Some("waiting"), Some("hook"), Some(105));
        assert!(set_screen_state(&connection, SESSION, CODER, "done", None, 111, latest).unwrap());
        let unset = (None, None, None);
        assert!(
            set_screen_state(&connection, SESSION, ORCHESTRATOR, "done", None, 112, unset).unwrap()
        );
    }

    #[test]
    fn a_hook_in_the_same_second_as_the_read_still_stops_the_screen_write() {
        let connection = seed(0);
        set_screen_state(
            &connection,
            SESSION,
            CODER,
            "working",
            None,
            100,
            (None, None, None),
        )
        .unwrap();
        // The listing read (working, screen, 100); a hook reports waiting in that same second.
        set_state(&connection, SESSION, CODER, "waiting", "hook", None, 100).unwrap();
        let read = (Some("working"), Some("screen"), Some(100));
        assert!(!set_screen_state(&connection, SESSION, CODER, "done", None, 100, read).unwrap());
        let coder = agents(&connection, SESSION)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.state.as_deref(), Some("waiting"));
    }

    #[test]
    fn closing_a_pane_clears_the_agents_state() {
        let connection = seed(0);
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_state(
            &connection,
            SESSION,
            CODER,
            "failed",
            "hook",
            Some("quota"),
            42,
        )
        .unwrap();
        clear_pane(&connection, SESSION, CODER).unwrap();
        let coder = agents(&connection, SESSION)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.pane, None);
        assert_eq!(
            (
                coder.state,
                coder.state_at,
                coder.state_source,
                coder.state_detail
            ),
            (None, None, None, None)
        );
    }

    /// A ring from an older build was never checked, and its pane has moved on since, so the
    /// migration stores it as unchecked. A later pass neither settles it nor reports it lost to
    /// the chair (ADR 0041).
    #[test]
    fn a_version_four_ring_is_unchecked_and_never_reported_lost() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..4] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 4).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role)
                     VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'),
                            ('{CODER}', '{SESSION}', 'coder');
                 INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path,
                                      rung_at, rings)
                     VALUES ('{SESSION}', 0, '{ORCHESTRATOR}', '{CODER}', 'ask', 'runs/0.txt',
                             1700, 2),
                            ('{SESSION}', 1, '{ORCHESTRATOR}', '{CODER}', 'ask', 'runs/1.txt',
                             NULL, 0);"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        assert!(unsettled_rings(&connection, SESSION).unwrap().is_empty());
        assert!(
            unreported_lost(&connection, SESSION, ORCHESTRATOR)
                .unwrap()
                .is_empty()
        );
        let delivery: Vec<Option<String>> = messages(&connection, SESSION, -1)
            .unwrap()
            .into_iter()
            .map(|message| message.delivery)
            .collect();
        assert_eq!(delivery, [Some("unchecked".to_string()), None]);
    }

    /// `swarm send` takes any kind, so an older database can hold two hand-sent messages of one
    /// report kind. The migration keeps both, and only a new report of that kind is refused.
    #[test]
    fn a_version_four_database_with_two_hand_sent_report_kinds_migrates_and_keeps_both() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..4] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 4).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role)
                     VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'),
                            ('{CODER}', '{SESSION}', 'coder');
                 INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path)
                     VALUES ('{SESSION}', 0, '{CODER}', '{ORCHESTRATOR}', 'stall:x', 'runs/0.txt'),
                            ('{SESSION}', 1, '{CODER}', '{ORCHESTRATOR}', 'stall:x', 'runs/1.txt');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        let kept: Vec<(i64, String)> = messages(&connection, SESSION, -1)
            .unwrap()
            .into_iter()
            .map(|message| (message.seq, message.kind))
            .collect();
        assert_eq!(
            kept,
            [(0, "stall:x".to_string()), (1, "stall:x".to_string())]
        );
        let root = std::env::temp_dir().join(format!("swarm-store-{}", uuid::Uuid::now_v7()));
        let resend = send_message(
            &mut connection,
            &root,
            SESSION,
            CODER,
            ORCHESTRATOR,
            "stall:x",
            "again",
        );
        let _ = std::fs::remove_dir_all(&root);
        // `report` in main.rs reads this code as a report already sent.
        assert!(matches!(
            resend.unwrap_err().downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(failure, _))
                if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER
        ));
    }

    #[test]
    fn a_version_three_database_gains_a_chat_log_that_outlives_the_pane() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..3] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 3).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role) VALUES ('{CODER}', '{SESSION}', 'coder');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].log, None);
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_log(&connection, SESSION, CODER, Path::new("/logs/coder.jsonl")).unwrap();
        clear_pane(&connection, SESSION, CODER).unwrap();
        assert_eq!(
            agents(&connection, SESSION).unwrap()[0].log.as_deref(),
            Some("/logs/coder.jsonl")
        );
    }

    #[test]
    fn rejects_database_from_another_build() {
        let root = temp_root("foreign-version");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 11")
            .unwrap();
        drop(connection);

        assert_eq!(
            open(&db).unwrap_err().to_string(),
            "database made by another swarm build; use another SWARM_HOME or delete it"
        );
        Connection::open(&db)
            .unwrap()
            .execute_batch("PRAGMA user_version = 7")
            .unwrap();
        assert!(open(&db).is_err());
    }

    /// The probe that decides adoption writes nothing, also for a WAL db, which a read-only
    /// open would give `-shm` and `-wal` files (ADR 0036).
    #[test]
    fn the_adoption_probe_leaves_another_programs_wal_database_as_it_was() {
        let root = temp_root("other-program-wal");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE bookmark (url TEXT);")
            .unwrap();
        let listing = || {
            let mut names: Vec<_> = std::fs::read_dir(&root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let (names, bytes) = (listing(), std::fs::read(&db).unwrap());

        assert!(!made_by_swarm(&db));
        assert_eq!(listing(), names);
        // A path with a leading `//` or a relative one names the same db.
        let mine = root.join("mine.db");
        open(&mine).unwrap();
        let double = std::path::PathBuf::from(format!("/{}", mine.display()));
        let relative = pathdiff_from_cwd(&mine);
        for path in [&mine, &double, &relative] {
            assert!(made_by_swarm(path), "{}", path.display());
        }
        assert!(open(&db).is_err());
        assert_eq!(std::fs::read(&db).unwrap(), bytes);
    }

    /// `path` written relative to the current directory, through `..` up to `/`.
    fn pathdiff_from_cwd(path: &Path) -> std::path::PathBuf {
        let cwd = std::env::current_dir().unwrap();
        let up = cwd.components().count() - 1;
        let mut relative: std::path::PathBuf = std::iter::repeat_n("..", up).collect();
        relative.push(path.strip_prefix("/").unwrap());
        relative
    }

    #[test]
    fn another_programs_database_gets_no_swarm_tables() {
        let root = temp_root("other-program");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch("CREATE TABLE bookmark (url TEXT)")
            .unwrap();

        assert!(!made_by_swarm(&db));
        assert_eq!(
            open(&db).unwrap_err().to_string(),
            "database not made by swarm; use another SWARM_HOME or move it"
        );
        let tables: Vec<String> = Connection::open(&db)
            .unwrap()
            .prepare("SELECT name FROM sqlite_master")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(tables, ["bookmark"]);
        open(&root.join("new.db")).unwrap();
        assert!(made_by_swarm(&root.join("new.db")));
    }
}
