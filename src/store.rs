use std::path::Path;

use rusqlite::Connection;

pub fn open(path: &Path) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let mut connection = rusqlite::Connection::open(path)?;

    connection.execute_batch("PRAGMA journal_mode=WAL;")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    migrate(&mut connection)?;

    Ok(connection)
}

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001.sql")];

fn migrate(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;

    let version: usize = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        if index < version {
            continue;
        }

        if sql.is_empty() {
            panic!("Sql empty")
        }

        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index + 1)?;
    }

    tx.commit()?;

    Ok(())
}

pub fn enqueue_job(connection: &Connection, agent_id: &str, kind: &str) -> Result<i64, Box<dyn std::error::Error>> {
    connection.execute("INSERT INTO job (agent_id, kind) VALUES (?1, ?2)", [agent_id, kind])?;
    Ok(connection.last_insert_rowid())
}

pub fn claim_job(connection: &Connection, job_id: i64) -> Result<bool, Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE job SET state = 'running', attempts = attempts + 1
         WHERE id = ?1 AND state = 'queued' AND run_after <= unixepoch()",
        [job_id],
    )?;
    Ok(changed == 1)
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
    std::fs::write(file, body)?;
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

pub fn create_session(connection: &Connection, talk_mode: &str) -> Result<i64, Box<dyn std::error::Error>> {
    connection.execute("INSERT INTO session (talk_mode) VALUES (?1)", [talk_mode])?;
    Ok(connection.last_insert_rowid())
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

pub fn ack(connection: &Connection, seq: i64, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "INSERT INTO read_mark (message_seq, agent_id) VALUES (?1, ?2)
         ON CONFLICT (message_seq, agent_id) DO NOTHING",
        (seq, agent_id),
    )?;
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
                "INSERT INTO session VALUES ({SESSION}, 'lane'), ({OTHER_SESSION}, 'lane');
                 INSERT INTO agent VALUES ('{ORCHESTRATOR}', {SESSION}, 'orchestrator'), ('{CODER}', {SESSION}, 'coder'), ('{OUTSIDER}', {OTHER_SESSION}, 'coder');
                 INSERT INTO job (id, agent_id, kind, run_after) VALUES (7, '{CODER}', 'build', {run_after});"
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
        assert!(claim_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("running".into(), 1));
    }

    #[test]
    fn rejects_missing_claimed_and_future_jobs() {
        let connection = seed(0);
        assert!(!claim_job(&connection, 8).unwrap());
        assert!(claim_job(&connection, 7).unwrap());
        assert!(!claim_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("running".into(), 1));

        let future = seed(i64::MAX);
        assert!(!claim_job(&future, 7).unwrap());
        assert_eq!(state_and_attempts(&future), ("queued".into(), 0));
    }

    #[test]
    fn returns_error_without_job_table() {
        let bare = Connection::open_in_memory().unwrap();
        assert!(claim_job(&bare, 7).is_err());
    }

    #[test]
    fn finishes_running_job_once() {
        let connection = seed(0);
        assert!(!finish_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 0));

        assert!(claim_job(&connection, 7).unwrap());
        assert!(finish_job(&connection, 7).unwrap());
        assert!(!finish_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("done".into(), 1));
    }

    #[test]
    fn releases_running_job_with_delay() {
        let connection = seed(0);
        assert!(!release_job(&connection, 7, 60).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 0));

        assert!(claim_job(&connection, 7).unwrap());
        assert!(release_job(&connection, 7, 60).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 1));
        assert!(!claim_job(&connection, 7).unwrap());

        let retry = seed(0);
        assert!(claim_job(&retry, 7).unwrap());
        assert!(release_job(&retry, 7, 0).unwrap());
        assert!(claim_job(&retry, 7).unwrap());
        assert_eq!(state_and_attempts(&retry), ("running".into(), 2));
    }

    #[test]
    fn parks_running_job_for_good() {
        let connection = seed(0);
        assert!(!park_job(&connection, 7).unwrap());
        assert_eq!(state_and_attempts(&connection), ("queued".into(), 0));

        assert!(claim_job(&connection, 7).unwrap());
        assert!(park_job(&connection, 7).unwrap());
        assert!(!claim_job(&connection, 7).unwrap());
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

        assert!(send_message(&mut connection, &root, SESSION, ORCHESTRATOR, OUTSIDER, "note", "x").is_err());
        let count: i64 = connection
            .query_row("SELECT count(*) FROM message", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!root.join("runs/1/2.txt").exists());
    }

    #[test]
    fn inbox_lists_pending_in_seq_order() {
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
        assert_eq!(inbox(&connection, SESSION, ORCHESTRATOR).unwrap().len(), 1);
        assert!(inbox(&connection, OTHER_SESSION, OUTSIDER).unwrap().is_empty());
    }

    #[test]
    fn ack_marks_once_and_only_for_recipient() {
        let mut connection = seed(0);
        let root = temp_root("ack");
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "one").unwrap();
        send_message(&mut connection, &root, SESSION, ORCHESTRATOR, CODER, "note", "two").unwrap();

        assert!(ack(&connection, 1, ORCHESTRATOR).is_err());
        assert_eq!(inbox(&connection, SESSION, CODER).unwrap().len(), 2);

        ack(&connection, 1, CODER).unwrap();
        ack(&connection, 1, CODER).unwrap();
        let marks: i64 = connection
            .query_row("SELECT count(*) FROM read_mark", [], |r| r.get(0))
            .unwrap();
        assert_eq!(marks, 1);
        assert_eq!(inbox(&connection, SESSION, CODER).unwrap()[0].seq, 2);
    }

    #[test]
    fn creates_sessions_with_increasing_ids() {
        let connection = seed(0);
        assert_eq!(create_session(&connection, "relay").unwrap(), 3);
        assert_eq!(create_session(&connection, "open").unwrap(), 4);
        assert!(create_session(&connection, "loud").is_err());
        let count: i64 = connection
            .query_row("SELECT count(*) FROM session", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn adds_agent_once_per_known_session() {
        let connection = seed(0);
        add_agent(&connection, SESSION, "reviewer", "reviewer").unwrap();
        assert!(add_agent(&connection, SESSION, "reviewer", "reviewer").is_err());
        assert!(add_agent(&connection, 99, "ghost", "coder").is_err());
        let count: i64 = connection
            .query_row("SELECT count(*) FROM agent", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn two_drainers_claim_every_job_once() {
        let db = temp_root("drain").join("swarm.db");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let connection = open(&db).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session VALUES ({SESSION}, 'lane');
                 INSERT INTO agent VALUES ('{CODER}', {SESSION}, 'coder');"
            ))
            .unwrap();
        for _ in 0..200 {
            enqueue_job(&connection, CODER, "summarize").unwrap();
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
}
