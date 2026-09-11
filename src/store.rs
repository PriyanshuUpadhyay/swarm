use std::path::Path;

use rusqlite::Connection;

pub fn open(path: &Path) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let mut connection = rusqlite::Connection::open(path)?;

    connection.execute_batch("PRAGMA journal_mode=WAL;")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut connection)?;

    Ok(connection)
}

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001.sql")];

fn migrate(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;

    let mut version: usize = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    println!("User version: {}", version);

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

    version = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    println!("User version: {}", version);

    Ok(())
}

pub fn claim_job(connection: &Connection, job_id: i64) -> Result<bool, Box<dyn std::error::Error>> {
    let changed = connection.execute(
        "UPDATE job SET state = 'running', attempts = attempts + 1
         WHERE id = ?1 AND state = 'queued' AND run_after <= unixepoch()",
        [job_id],
    )?;
    Ok(changed == 1)
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
    session_id: &str,
    sender_id: &str,
    recipient_id: &str,
    kind: &str,
    body: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO message (session_id, sender_id, recipient_id, kind, body_path)
         VALUES (?1, ?2, ?3, ?4, '')",
        [session_id, sender_id, recipient_id, kind],
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(run_after: i64) -> Connection {
        let connection = open(Path::new(":memory:")).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session VALUES ('s', 'lane');
                 INSERT INTO agent VALUES ('a', 's', 'worker');
                 INSERT INTO job (id, agent_id, kind, run_after) VALUES (7, 'a', 'build', {run_after});"
            ))
            .unwrap();
        connection
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
}
