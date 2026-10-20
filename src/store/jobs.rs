//! Job queue claims and state changes.

use super::*;

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

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

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
}
