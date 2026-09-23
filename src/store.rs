use std::path::Path;

use rusqlite::{Connection, TransactionBehavior};

pub fn open(path: &Path) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let mut connection = rusqlite::Connection::open(path)?;

    connection.execute_batch("PRAGMA journal_mode=WAL;")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    migrate(&mut connection)?;
    connection.pragma_update(None, "foreign_keys", true)?;

    Ok(connection)
}

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001.sql")];

fn migrate(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;

    let version: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 {
        tx.execute_batch(MIGRATIONS[0])?;
        tx.pragma_update(None, "user_version", 1)?;
    } else if version != 1 {
        return Err("database made by another swarm build; use another SWARM_HOME or delete it".into());
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
pub fn job(connection: &Connection, job_id: i64) -> Result<(String, String, String, i64), Box<dyn std::error::Error>> {
    let row = connection.query_row(
        "SELECT session_id, agent_id, kind, attempts FROM job WHERE id = ?1",
        [job_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(row)
}

pub fn finish_job(connection: &Connection, job_id: i64) -> Result<bool, Box<dyn std::error::Error>> {
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
        Ok(Pending { seq: r.get(0)?, sender_id: r.get(1)?, kind: r.get(2)?, body_path: r.get(3)? })
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
    pub chair_days: [String; 3],
    pub agents: i64,
    pub messages: i64,
    pub last_message_at: Option<i64>,
}

pub fn sessions(connection: &Connection) -> Result<Vec<SessionRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT session.id, talk_mode, adapter, cwd, session.created_at,
                chair_provider, chair_id, chair_log,
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
            chair_days: [row.get(8)?, row.get(9)?, row.get(10)?],
            agents: row.get(11)?,
            messages: row.get(12)?,
            last_message_at: row.get(13)?,
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

pub fn set_provider(connection: &Connection, session_id: &str, agent_id: &str, provider: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    let moves_orchestrator = orchestrator_of(connection, session_id)
        .is_ok_and(|orchestrator| orchestrator == agent_id);
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

#[derive(Debug)]
pub struct AgentRow {
    pub id: String,
    pub role: String,
    pub pane: Option<String>,
    pub provider: Option<String>,
    pub created_at: i64,
}

pub fn agents(connection: &Connection, session_id: &str) -> Result<Vec<AgentRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT id, role, pane_id, provider, created_at FROM agent WHERE session_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([session_id], |row| {
        Ok(AgentRow { id: row.get(0)?, role: row.get(1)?, pane: row.get(2)?, provider: row.get(3)?, created_at: row.get(4)? })
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
                          AND message_seq = message.seq AND agent_id = message.recipient_id)
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
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
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
             AND MAX(rings) < 2
         FROM message
         WHERE session_id = ?1 AND recipient_id = ?2
           AND seen_at IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM read_mark
               WHERE read_mark.session_id = message.session_id
                 AND message_seq = message.seq AND agent_id = ?2
           )",
        (session_id, agent_id, age_secs),
        |row| row.get(0),
    )?;
    Ok(due)
}

pub fn clear_pane(connection: &Connection, session_id: &str, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET pane_id = NULL WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id),
    )?;
    Ok(())
}

/// Session agents that have a pane, except the caller, as (agent_id, pane_id) by id.
pub fn live_children(connection: &Connection, session_id: &str, except: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT id, pane_id FROM agent WHERE session_id = ?1 AND pane_id IS NOT NULL AND id != ?2 ORDER BY id",
    )?;
    let rows = statement.query_map((session_id, except), |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn has_summary(connection: &Connection, session_id: &str, sender_id: &str) -> Result<bool, Box<dyn std::error::Error>> {
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
    let mode: String = connection.query_row("SELECT talk_mode FROM session WHERE id = ?1", [session_id], |r| r.get(0))?;
    let orchestrator = orchestrator_of(connection, session_id)?;
    if mode == "open" || sender == orchestrator || recipient == orchestrator {
        return Ok((recipient.to_string(), kind.to_string()));
    }
    if mode == "lane" {
        return Err(format!("lane: {sender} cannot message {recipient}").into());
    }
    Ok((orchestrator, format!("relay:{recipient}")))
}

pub fn orchestrator_of(connection: &Connection, session_id: &str) -> Result<String, Box<dyn std::error::Error>> {
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

pub fn ack(connection: &Connection, session_id: &str, seq: i64, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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
        return Err(format!("message {seq} is not for agent {agent_id} in session {session_id}").into());
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
        assert_eq!(job(&connection, 7).unwrap(), (SESSION.to_string(), CODER.to_string(), "build".to_string(), 1));
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
        let seq = send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "hello").unwrap();
        assert_eq!(seq, 0);
        let body_path: String = connection
            .query_row("SELECT body_path FROM message WHERE seq = 0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(body_path, format!("runs/{SESSION}/0.txt"));
        assert_eq!(std::fs::read_to_string(root.join(body_path)).unwrap(), "hello");
        assert!(!root.join(format!("runs/{SESSION}/0.tmp")).exists());

        assert!(send_message(&mut connection, &root, SESSION, ORCHESTRATOR, OUTSIDER, "note", "x").is_err());
        let count: i64 = connection
            .query_row("SELECT count(*) FROM message", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!root.join(&format!("runs/{SESSION}/1.txt")).exists());
    }

    #[test]
    fn inbox_lists_pending_in_seq_order_and_stamps_seen_once() {
        let mut connection = seed(0);
        let root = temp_root("inbox");
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "one").unwrap();
        send_message(&mut connection, &root, SESSION, CODER, ORCHESTRATOR, "note", "reply").unwrap();
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "ask", "two").unwrap();

        let pending = inbox(&connection, SESSION, CODER).unwrap();
        let seen: Vec<(i64, &str, &str, &str)> = pending
            .iter()
            .map(|m| (m.seq, m.sender_id.as_str(), m.kind.as_str(), m.body_path.as_str()))
            .collect();
        assert_eq!(seen, [(0, ORCHESTRATOR, "note", format!("runs/{SESSION}/0.txt").as_str()), (2, ORCHESTRATOR, "ask", format!("runs/{SESSION}/2.txt").as_str())]);
        let seen_at: i64 = connection.query_row("SELECT seen_at FROM message WHERE seq = 0", [], |r| r.get(0)).unwrap();
        assert!(seen_at > 0);
        connection.execute("UPDATE message SET seen_at = 7 WHERE seq = 0", []).unwrap();
        inbox(&connection, SESSION, CODER).unwrap();
        assert_eq!(connection.query_row("SELECT seen_at FROM message WHERE seq = 0", [], |r| r.get::<_, i64>(0)).unwrap(), 7);
        assert_eq!(inbox(&connection, SESSION, ORCHESTRATOR).unwrap().len(), 1);
        assert!(inbox(&connection, OTHER_SESSION, OUTSIDER).unwrap().is_empty());
    }

    #[test]
    fn ack_marks_once_and_only_for_recipient() {
        let mut connection = seed(0);
        let root = temp_root("ack");
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "one").unwrap();
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "two").unwrap();

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
        assert_eq!(pane_of(&connection, &first, ORCHESTRATOR).unwrap().as_deref(), Some("%1"));
        assert_eq!(pane_of(&connection, &second, ORCHESTRATOR).unwrap().as_deref(), Some("%2"));
    }

    #[test]
    fn an_orchestrator_pane_moves_only_within_one_adapter() {
        let connection = open(Path::new(":memory:")).unwrap();
        let tmux = create_session(&connection, "lane", Path::new("/tmux"), None, Some("tmux")).unwrap();
        let same = create_session(&connection, "lane", Path::new("/same"), None, Some("tmux")).unwrap();
        let solo = create_session(&connection, "lane", Path::new("/solo"), None, Some("tmux-solo")).unwrap();
        for session in [&tmux, &same, &solo] {
            add_agent(&connection, session, ORCHESTRATOR, "code.complex").unwrap();
        }

        set_pane(&connection, &tmux, ORCHESTRATOR, "%0").unwrap();
        set_pane(&connection, &solo, ORCHESTRATOR, "%0").unwrap();
        assert_eq!(pane_of(&connection, &tmux, ORCHESTRATOR).unwrap().as_deref(), Some("%0"));
        assert_eq!(pane_of(&connection, &solo, ORCHESTRATOR).unwrap().as_deref(), Some("%0"));

        set_pane(&connection, &same, ORCHESTRATOR, "%0").unwrap();
        assert_eq!(pane_of(&connection, &tmux, ORCHESTRATOR).unwrap(), None);
        assert_eq!(pane_of(&connection, &same, ORCHESTRATOR).unwrap().as_deref(), Some("%0"));
        assert_eq!(pane_of(&connection, &solo, ORCHESTRATOR).unwrap().as_deref(), Some("%0"));
    }

    /// A stale adapter answered `self` with nothing. The empty answer was stored, and the chat
    /// said `no pane recorded` at the first message rather than at registration.
    #[test]
    fn an_empty_pane_is_refused() {
        let connection = seed(0);
        let refused = set_pane(&connection, SESSION, ORCHESTRATOR, "  ").unwrap_err().to_string();
        assert_eq!(refused, "swarm: adapter gave no pane for orchestrator");
        assert_eq!(pane_of(&connection, SESSION, ORCHESTRATOR).unwrap(), None);
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(pane_of(&connection, SESSION, ORCHESTRATOR).unwrap().as_deref(), Some("%1"));
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
            .query_row("SELECT sum(state = 'done'), sum(attempts = 1) FROM job", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((done, once), (200, 200));
    }

    #[test]
    fn sets_and_reads_pane() {
        let connection = seed(0);
        assert_eq!(pane_of(&connection, SESSION, CODER).unwrap(), None);
        set_pane(&connection, SESSION, CODER, "w8A:p2").unwrap();
        assert_eq!(pane_of(&connection, SESSION, CODER).unwrap().as_deref(), Some("w8A:p2"));
        assert!(pane_of(&connection, OTHER_SESSION, CODER).is_err());
        assert!(pane_of(&connection, SESSION, "ghost").is_err());
    }

    #[test]
    fn rings_only_unread_and_rerings_once_until_seen() {
        let mut connection = seed(0);
        let root = temp_root("old-unread");
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "ask", "old").unwrap();
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "ask", "new").unwrap();
        connection.execute("UPDATE message SET created_at = unixepoch() - 61 WHERE seq = 0", []).unwrap();

        assert!(!has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 62).unwrap());
        assert!(!rering_due(&connection, OTHER_SESSION, OUTSIDER, 60).unwrap());
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection.execute("UPDATE message SET rung_at = unixepoch(), rings = 1 WHERE seq = 0", []).unwrap();
        assert!(has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection.execute("UPDATE message SET rung_at = unixepoch() - 61 WHERE seq = 0", []).unwrap();
        assert!(rering_due(&connection, SESSION, CODER, 60).unwrap());
        connection.execute("UPDATE message SET rings = 2 WHERE seq = 0", []).unwrap();
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        inbox(&connection, SESSION, CODER).unwrap();
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
        ack(&connection, SESSION, 0, CODER).unwrap();
        assert!(!has_rung_unread(&connection, SESSION, CODER).unwrap());
        assert!(!rering_due(&connection, SESSION, CODER, 60).unwrap());
    }

    #[test]
    fn finds_the_session_orchestrator() {
        let connection = seed(0);
        assert_eq!(orchestrator_of(&connection, SESSION).unwrap(), ORCHESTRATOR);
        connection.execute(
            "UPDATE agent SET role = 'code.complex' WHERE session_id = ?1 AND id = ?2",
            (SESSION, ORCHESTRATOR),
        ).unwrap();
        assert_eq!(orchestrator_of(&connection, SESSION).unwrap(), ORCHESTRATOR);
        assert_eq!(orchestrator_of(&connection, OTHER_SESSION).unwrap_err().to_string(), format!("session {OTHER_SESSION} has no orchestrator"));
    }

    #[test]
    fn lists_live_children_and_summaries() {
        let mut connection = seed(0);
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_pane(&connection, SESSION, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(live_children(&connection, SESSION, ORCHESTRATOR).unwrap(), [(CODER.to_string(), "%2".to_string())]);
        clear_pane(&connection, SESSION, CODER).unwrap();
        assert!(live_children(&connection, SESSION, ORCHESTRATOR).unwrap().is_empty());
        assert!(!has_summary(&connection, SESSION, CODER).unwrap());
        send_message(&mut connection, &temp_root("summary"), SESSION, CODER, ORCHESTRATOR, "summary", "done").unwrap();
        assert!(has_summary(&connection, SESSION, CODER).unwrap());
    }

    #[test]
    fn routes_by_talk_mode() {
        let connection = seed(0);
        connection.execute_batch(&format!("INSERT INTO agent (id, session_id, role) VALUES ('reviewer', '{SESSION}', 'reviewer')")).unwrap();
        let set_mode = |mode: &str| connection.execute("UPDATE session SET talk_mode = ?1 WHERE id = ?2", (mode, SESSION)).unwrap();
        assert_eq!(route(&connection, SESSION, CODER, "reviewer", "ask").unwrap_err().to_string(), "lane: coder cannot message reviewer");
        assert_eq!(route(&connection, SESSION, CODER, ORCHESTRATOR, "ask").unwrap(), (ORCHESTRATOR.into(), "ask".into()));
        set_mode("relay");
        assert_eq!(route(&connection, SESSION, CODER, "reviewer", "ask").unwrap(), (ORCHESTRATOR.into(), "relay:reviewer".into()));
        set_mode("open");
        assert_eq!(route(&connection, SESSION, CODER, "reviewer", "ask").unwrap(), ("reviewer".into(), "ask".into()));
    }

    #[test]
    fn session_ids_are_never_reused() {
        let connection = open(Path::new(":memory:")).unwrap();
        let first = create_session(&connection, "lane", Path::new("/first"), None, None).unwrap();
        connection.execute("DELETE FROM session WHERE id = ?1", [&first]).unwrap();
        assert_ne!(create_session(&connection, "lane", Path::new("/second"), None, None).unwrap(), first);
    }

    #[test]
    fn rejects_database_from_another_build() {
        let root = temp_root("foreign-version");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        let connection = Connection::open(&db).unwrap();
        connection.execute_batch("PRAGMA user_version = 11").unwrap();
        drop(connection);

        assert_eq!(open(&db).unwrap_err().to_string(),
                   "database made by another swarm build; use another SWARM_HOME or delete it");
    }
}
