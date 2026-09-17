use std::path::Path;

use rusqlite::Connection;

pub fn open(path: &Path) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let mut connection = rusqlite::Connection::open(path)?;

    connection.execute_batch("PRAGMA journal_mode=WAL;")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    // Migrate with foreign keys off: a table rebuild drops a table that other rows reference.
    connection.pragma_update(None, "foreign_keys", false)?;
    migrate(&mut connection)?;
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
    include_str!("../migrations/0007.sql"),
    include_str!("../migrations/0008.sql"),
    include_str!("../migrations/0009.sql"),
];

fn migrate(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;

    let version: usize = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        if index < version {
            continue;
        }

        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index + 1)?;
    }

    tx.commit()?;

    Ok(())
}

pub fn enqueue_job(
    connection: &Connection,
    session_id: i64,
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
pub fn job(connection: &Connection, job_id: i64) -> Result<(i64, String, String, i64), Box<dyn std::error::Error>> {
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
    session_id: i64,
    sender_id: &str,
    recipient_id: &str,
    kind: &str,
    body: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO message (session_id, sender_id, recipient_id, kind, body_path)
         VALUES (?1, ?2, ?3, ?4, '')",
        (session_id, sender_id, recipient_id, kind),
    )?;
    let seq = tx.last_insert_rowid();
    let body_path = format!("runs/{session_id}/{seq}.txt");
    tx.execute("UPDATE message SET body_path = ?1 WHERE seq = ?2", (&body_path, seq))?;
    let file = root.join(body_path);
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
    session_id: i64,
    agent_id: &str,
) -> Result<Vec<Pending>, Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE message SET seen_at = unixepoch()
         WHERE session_id = ?1 AND recipient_id = ?2 AND seen_at IS NULL
           AND seq NOT IN (SELECT message_seq FROM read_mark WHERE agent_id = ?2)",
        (session_id, agent_id),
    )?;
    let mut statement = connection.prepare(
        "SELECT seq, sender_id, kind, body_path FROM message
         WHERE session_id = ?1 AND recipient_id = ?2
           AND seq NOT IN (SELECT message_seq FROM read_mark WHERE agent_id = ?2)
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
) -> Result<i64, Box<dyn std::error::Error>> {
    let cwd = cwd.to_string_lossy().into_owned();
    let (chair_provider, chair_id) = chair.unzip();
    connection.execute(
        "INSERT INTO session (talk_mode, cwd, created_at, adapter, chair_provider, chair_id)
         VALUES (?1, ?2, unixepoch(), ?3, ?4, ?5)",
        (talk_mode, cwd, adapter, chair_provider, chair_id),
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn set_chair(
    connection: &Connection,
    session_id: i64,
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
    session_id: i64,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE session SET chair_log = ?2 WHERE id = ?1",
        (session_id, path.to_string_lossy()),
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct SessionRow {
    pub id: i64,
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
         WHERE cwd IS NOT NULL
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
    session_id: i64,
    agent_id: &str,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "INSERT INTO agent (id, session_id, role) VALUES (?1, ?2, ?3)",
        (agent_id, session_id, role),
    )?;
    Ok(())
}

pub fn set_pane(
    connection: &Connection,
    session_id: i64,
    agent_id: &str,
    pane_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET pane_id = ?1 WHERE session_id = ?2 AND id = ?3",
        (pane_id, session_id, agent_id),
    )?;
    Ok(())
}

pub fn pane_of(
    connection: &Connection,
    session_id: i64,
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
}

pub fn agents(connection: &Connection, session_id: i64) -> Result<Vec<AgentRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT id, role, pane_id FROM agent WHERE session_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([session_id], |row| {
        Ok(AgentRow { id: row.get(0)?, role: row.get(1)?, pane: row.get(2)? })
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
    session_id: i64,
    after: i64,
) -> Result<Vec<MessageRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT message.seq, sender_id, recipient_id, kind, body_path, created_at,
                EXISTS (SELECT 1 FROM read_mark
                        WHERE message_seq = message.seq AND agent_id = message.recipient_id)
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

pub fn mark_unseen_for_rering(
    connection: &Connection,
    session_id: i64,
    agent_id: &str,
    age_secs: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE message SET rung_at = unixepoch()
         WHERE session_id = ?1 AND recipient_id = ?2
           AND created_at <= unixepoch() - ?3
           AND (rung_at IS NULL OR rung_at <= unixepoch() - ?3)
           AND seen_at IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM read_mark
               WHERE message_seq = message.seq AND agent_id = ?2
           )",
        (session_id, agent_id, age_secs),
    )?;
    Ok(changed > 0)
}

pub fn clear_pane(connection: &Connection, session_id: i64, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET pane_id = NULL WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id),
    )?;
    Ok(())
}

/// Session agents that have a pane, except the caller, as (agent_id, pane_id) by id.
pub fn live_children(connection: &Connection, session_id: i64, except: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT id, pane_id FROM agent WHERE session_id = ?1 AND pane_id IS NOT NULL AND id != ?2 ORDER BY id",
    )?;
    let rows = statement.query_map((session_id, except), |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn has_summary(connection: &Connection, session_id: i64, sender_id: &str) -> Result<bool, Box<dyn std::error::Error>> {
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
    session_id: i64,
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

pub fn orchestrator_of(connection: &Connection, session_id: i64) -> Result<String, Box<dyn std::error::Error>> {
    let id = connection
        .query_row("SELECT id FROM agent WHERE session_id = ?1 AND role = 'orchestrator'", [session_id], |r| r.get(0))
        .map_err(|_| format!("session {session_id} has no orchestrator"))?;
    Ok(id)
}

pub fn ack(connection: &Connection, session_id: i64, seq: i64, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "INSERT INTO read_mark (message_seq, agent_id)
         SELECT seq, recipient_id FROM message
         WHERE session_id = ?1 AND seq = ?2 AND recipient_id = ?3
         ON CONFLICT (message_seq, agent_id) DO NOTHING",
        (session_id, seq, agent_id),
    )?;
    if changed == 0
        && !connection.query_row(
            "SELECT EXISTS (
             SELECT 1 FROM read_mark
             JOIN message ON message.seq = read_mark.message_seq
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

    const SESSION: i64 = 1;
    const OTHER_SESSION: i64 = 2;
    const ORCHESTRATOR: &str = "orchestrator";
    const CODER: &str = "coder";
    const OUTSIDER: &str = "outsider";

    fn seed(run_after: i64) -> Connection {
        let connection = open(Path::new(":memory:")).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode) VALUES ({SESSION}, 'lane'), ({OTHER_SESSION}, 'lane');
                 INSERT INTO agent (id, session_id, role) VALUES ('{ORCHESTRATOR}', {SESSION}, 'orchestrator'), ('{CODER}', {SESSION}, 'coder'), ('{OUTSIDER}', {OTHER_SESSION}, 'coder');
                 INSERT INTO job (id, session_id, agent_id, kind, run_after) VALUES (7, {SESSION}, '{CODER}', 'build', {run_after});"
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
        assert_eq!(job(&connection, 7).unwrap(), (SESSION, CODER.to_string(), "build".to_string(), 1));
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
        assert_eq!(seq, 1);
        let body_path: String = connection
            .query_row("SELECT body_path FROM message WHERE seq = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(body_path, "runs/1/1.txt");
        assert_eq!(std::fs::read_to_string(root.join(body_path)).unwrap(), "hello");
        assert!(!root.join("runs/1/1.tmp").exists());

        assert!(send_message(&mut connection, &root, SESSION, ORCHESTRATOR, OUTSIDER, "note", "x").is_err());
        let count: i64 = connection
            .query_row("SELECT count(*) FROM message", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!root.join("runs/1/2.txt").exists());
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
        assert_eq!(seen, [(1, ORCHESTRATOR, "note", "runs/1/1.txt"), (3, ORCHESTRATOR, "ask", "runs/1/3.txt")]);
        let seen_at: i64 = connection.query_row("SELECT seen_at FROM message WHERE seq = 1", [], |r| r.get(0)).unwrap();
        assert!(seen_at > 0);
        connection.execute("UPDATE message SET seen_at = 7 WHERE seq = 1", []).unwrap();
        inbox(&connection, SESSION, CODER).unwrap();
        assert_eq!(connection.query_row("SELECT seen_at FROM message WHERE seq = 1", [], |r| r.get::<_, i64>(0)).unwrap(), 7);
        assert_eq!(inbox(&connection, SESSION, ORCHESTRATOR).unwrap().len(), 1);
        assert!(inbox(&connection, OTHER_SESSION, OUTSIDER).unwrap().is_empty());
    }

    #[test]
    fn ack_marks_once_and_only_for_recipient() {
        let mut connection = seed(0);
        let root = temp_root("ack");
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "one").unwrap();
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "two").unwrap();

        assert!(ack(&connection, SESSION, 1, ORCHESTRATOR).is_err());
        assert!(ack(&connection, OTHER_SESSION, 1, CODER).is_err());
        assert_eq!(inbox(&connection, SESSION, CODER).unwrap().len(), 2);

        ack(&connection, SESSION, 1, CODER).unwrap();
        ack(&connection, SESSION, 1, CODER).unwrap();
        let marks: i64 = connection
            .query_row("SELECT count(*) FROM read_mark", [], |r| r.get(0))
            .unwrap();
        assert_eq!(marks, 1);
        assert_eq!(inbox(&connection, SESSION, CODER).unwrap()[0].seq, 2);
    }

    #[test]
    fn creates_sessions_with_increasing_ids() {
        let connection = seed(0);
        assert_eq!(create_session(&connection, "relay", Path::new("/relay"), None, None).unwrap(), 3);
        assert_eq!(create_session(&connection, "open", Path::new("/open"), None, None).unwrap(), 4);
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
        add_agent(&connection, first, ORCHESTRATOR, "orchestrator").unwrap();
        add_agent(&connection, second, ORCHESTRATOR, "orchestrator").unwrap();
        set_pane(&connection, first, ORCHESTRATOR, "%1").unwrap();
        set_pane(&connection, second, ORCHESTRATOR, "%2").unwrap();
        assert!(add_agent(&connection, second, ORCHESTRATOR, "orchestrator").is_err());
        assert!(add_agent(&connection, 99, "ghost", "coder").is_err());
        assert_eq!(pane_of(&connection, first, ORCHESTRATOR).unwrap().as_deref(), Some("%1"));
        assert_eq!(pane_of(&connection, second, ORCHESTRATOR).unwrap().as_deref(), Some("%2"));
    }

    #[test]
    fn two_drainers_claim_every_job_once() {
        let db = temp_root("drain").join("swarm.db");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let connection = open(&db).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode) VALUES ({SESSION}, 'lane');
                 INSERT INTO agent (id, session_id, role) VALUES ('{CODER}', {SESSION}, 'coder');"
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
    fn finds_only_old_unseen_messages() {
        let mut connection = seed(0);
        let root = temp_root("old-unread");
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "ask", "old").unwrap();
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "ask", "new").unwrap();
        connection.execute("UPDATE message SET created_at = unixepoch() - 61 WHERE seq = 1", []).unwrap();

        assert!(!mark_unseen_for_rering(&connection, SESSION, CODER, 62).unwrap());
        assert!(!mark_unseen_for_rering(&connection, OTHER_SESSION, OUTSIDER, 60).unwrap());
        assert!(mark_unseen_for_rering(&connection, SESSION, CODER, 60).unwrap());
        assert!(!mark_unseen_for_rering(&connection, SESSION, CODER, 60).unwrap());
        inbox(&connection, SESSION, CODER).unwrap();
        connection.execute("UPDATE message SET rung_at = unixepoch() - 61 WHERE seq = 1", []).unwrap();
        assert!(!mark_unseen_for_rering(&connection, SESSION, CODER, 60).unwrap());
        connection.execute("UPDATE message SET seen_at = NULL, rung_at = unixepoch() - 61 WHERE seq = 1", []).unwrap();
        ack(&connection, SESSION, 1, CODER).unwrap();
        assert!(!mark_unseen_for_rering(&connection, SESSION, CODER, 60).unwrap());
    }

    #[test]
    fn finds_the_session_orchestrator() {
        let connection = seed(0);
        assert_eq!(orchestrator_of(&connection, SESSION).unwrap(), ORCHESTRATOR);
        assert_eq!(orchestrator_of(&connection, OTHER_SESSION).unwrap_err().to_string(), "session 2 has no orchestrator");
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
        connection.execute_batch("INSERT INTO agent (id, session_id, role) VALUES ('reviewer', 1, 'reviewer')").unwrap();
        let set_mode = |mode: &str| connection.execute("UPDATE session SET talk_mode = ?1 WHERE id = 1", [mode]).unwrap();
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
        connection.execute("DELETE FROM session WHERE id = ?1", [first]).unwrap();
        assert_ne!(create_session(&connection, "lane", Path::new("/second"), None, None).unwrap(), first);
    }

    #[test]
    fn migration_adds_message_time_without_losing_read_marks() {
        let root = temp_root("message-migration");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        let connection = Connection::open(&db).unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection.execute_batch(MIGRATIONS[1]).unwrap();
        connection
            .execute_batch(
                "PRAGMA user_version = 2;
                 INSERT INTO session (id, talk_mode) VALUES (1, 'lane');
                 INSERT INTO agent (id, session_id, role) VALUES ('orchestrator', 1, 'orchestrator'), ('coder', 1, 'coder');
                 INSERT INTO message (seq, session_id, sender_id, recipient_id, kind, body_path)
                     VALUES (1, 1, 'orchestrator', 'coder', 'ask', 'runs/1/1.txt');
                 INSERT INTO read_mark (message_seq, agent_id) VALUES (1, 'coder');
                 INSERT INTO job (id, agent_id, kind, state, attempts, run_after)
                     VALUES (7, 'coder', 'build', 'running', 2, 10);",
            )
            .unwrap();
        drop(connection);

        let connection = open(&db).unwrap();

        let created_at: i64 = connection.query_row("SELECT created_at FROM message WHERE seq = 1", [], |r| r.get(0)).unwrap();
        assert!(created_at > 0);
        assert!(inbox(&connection, SESSION, CODER).unwrap().is_empty());
        assert_eq!(job(&connection, 7).unwrap(), (SESSION, CODER.to_string(), "build".to_string(), 2));
        assert_eq!(connection.query_row("SELECT count(*) FROM agent", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        assert_eq!(connection.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), 9);
        let metadata = connection
            .query_row("SELECT cwd, session.created_at, chair_log, adapter, chair_provider, chair_id FROM session WHERE id = 1", [], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })
            .unwrap();
        assert_eq!(metadata, (None, None, None, None, None, None));
        assert_eq!(connection.query_row("SELECT seen_at FROM message WHERE seq = 1", [], |r| r.get::<_, i64>(0)).unwrap(), created_at);
        assert!(!mark_unseen_for_rering(&connection, SESSION, CODER, 0).unwrap());
        let mut foreign_key_check = connection.prepare("PRAGMA foreign_key_check").unwrap();
        assert!(foreign_key_check.query([]).unwrap().next().unwrap().is_none());
    }

    #[test]
    fn migration_from_version_five_keeps_messages() {
        let root = temp_root("seen-migration");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        let connection = Connection::open(&db).unwrap();
        for migration in &MIGRATIONS[..5] {
            connection.execute_batch(migration).unwrap();
        }
        connection
            .execute_batch(
                "PRAGMA user_version = 5;
                 INSERT INTO session (id, talk_mode) VALUES (1, 'lane');
                 INSERT INTO agent (id, session_id, role) VALUES ('orchestrator', 1, 'orchestrator'), ('coder', 1, 'coder');
                 INSERT INTO message (seq, session_id, sender_id, recipient_id, kind, body_path)
                     VALUES (1, 1, 'orchestrator', 'coder', 'ask', 'runs/1/1.txt');",
            )
            .unwrap();
        drop(connection);

        let connection = open(&db).unwrap();

        assert_eq!(connection.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), 9);
        assert_eq!(connection.query_row("SELECT body_path FROM message WHERE seq = 1", [], |r| r.get::<_, String>(0)).unwrap(), "runs/1/1.txt");
        let (created_at, seen_at): (i64, i64) = connection
            .query_row("SELECT created_at, seen_at FROM message WHERE seq = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(seen_at, created_at);
    }
}
