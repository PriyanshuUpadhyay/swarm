//! Session records and chair settings.

use super::*;

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

/// Names the chair's session. A new one has a new log; a resumed one keeps its log, which a hook
/// that runs beside this one may have just recorded (ADR 0107).
pub fn set_chair(
    connection: &Connection,
    session_id: &str,
    chair: Option<(&str, &str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (provider, id) = chair.unzip();
    let changed = connection.execute(
        "UPDATE session SET chair_log = CASE WHEN chair_provider IS ?2 AND chair_id IS ?3
                                             THEN chair_log END,
                            chair_provider = ?2, chair_id = ?3
         WHERE id = ?1",
        (session_id, provider, id),
    )?;
    if changed != 1 {
        return Err(format!("session {session_id} not found").into());
    }
    Ok(())
}

/// Names the chair's CLI before its own id is known. A chair already named keeps its name.
pub fn set_chair_provider(
    connection: &Connection,
    session_id: &str,
    provider: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE session SET chair_provider = ?2 WHERE id = ?1 AND chair_provider IS NULL",
        (session_id, provider),
    )?;
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
    // A Codex rollout file ends in its thread id, which is the chair id that Codex never reports.
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    if let Some(thread) = stem.get(stem.len().saturating_sub(36)..)
        && uuid::Uuid::try_parse(thread).is_ok()
    {
        connection.execute(
            "UPDATE session SET chair_id = ?2
             WHERE id = ?1 AND chair_provider = 'codex' AND chair_id IS NULL",
            (session_id, thread),
        )?;
    }
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

pub fn adapter_of(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    Ok(connection.query_row(
        "SELECT adapter FROM session WHERE id = ?1",
        [session_id],
        |row| row.get(0),
    )?)
}

pub fn archive_sessions(
    connection: &mut Connection,
    session_ids: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    set_sessions_archived(connection, session_ids, true)
}

pub fn unarchive_sessions(
    connection: &mut Connection,
    session_ids: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    set_sessions_archived(connection, session_ids, false)
}

fn set_sessions_archived(
    connection: &mut Connection,
    session_ids: &[String],
    archived: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let tx = connection.transaction()?;
    for session_id in session_ids {
        let changed = tx.execute(
            "UPDATE session SET archived_at = CASE WHEN ?2 THEN unixepoch() ELSE NULL END
             WHERE id = ?1",
            (session_id, archived),
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
    /// `archived_at` in storage and `archivedAt` on the wire both use Unix seconds.
    pub archived_at: Option<i64>,
}

pub fn sessions(connection: &Connection) -> Result<Vec<SessionRow>, Box<dyn std::error::Error>> {
    sessions_with_archived(connection, false)
}

const ARCHIVED_SESSIONS_LIMIT: i64 = 50;

pub fn sessions_with_archived(
    connection: &Connection,
    include_archived: bool,
) -> Result<Vec<SessionRow>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT session.id, talk_mode, adapter, cwd, session.created_at,
                chair_provider, chair_id, chair_log, continuation_of,
                strftime('%Y/%m/%d', session.created_at - 86400, 'unixepoch'),
                strftime('%Y/%m/%d', session.created_at, 'unixepoch'),
                strftime('%Y/%m/%d', session.created_at + 86400, 'unixepoch'),
                (SELECT count(*) FROM agent WHERE session_id = session.id),
                (SELECT count(*) FROM message WHERE session_id = session.id),
                (SELECT max(created_at) FROM message WHERE session_id = session.id),
                archived_at
         FROM session
         WHERE archived_at IS NULL OR (?1 AND session.id IN (
             SELECT id FROM session WHERE archived_at IS NOT NULL
             ORDER BY archived_at DESC, id DESC LIMIT ?2
         ))
         ORDER BY archived_at IS NOT NULL, coalesce(archived_at, session.created_at) DESC,
                  session.id DESC",
    )?;
    let rows = statement.query_map((include_archived, ARCHIVED_SESSIONS_LIMIT), |row| {
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
            archived_at: row.get(15)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The folder a session was opened in, or None for an unknown session.
pub fn session_cwd(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row(
            "SELECT cwd FROM session WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn creates_sessions_with_unique_uuid_v7_ids() {
        let connection = seed();
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
    fn a_chair_named_again_keeps_its_log_and_a_new_one_drops_it() {
        let connection = open(Path::new(":memory:")).unwrap();
        let session = create_session(
            &connection,
            "lane",
            Path::new("/w"),
            Some(("claude", "a")),
            None,
        )
        .unwrap();
        let log = |connection: &Connection| -> Option<String> {
            connection
                .query_row(
                    "SELECT chair_log FROM session WHERE id = ?1",
                    [&session],
                    |r| r.get(0),
                )
                .unwrap()
        };
        set_chair_log(&connection, &session, Path::new("/logs/a.jsonl")).unwrap();
        set_chair(&connection, &session, Some(("claude", "a"))).unwrap();
        assert_eq!(
            log(&connection).as_deref(),
            Some("/logs/a.jsonl"),
            "a resume keeps the log"
        );
        set_chair(&connection, &session, Some(("claude", "b"))).unwrap();
        assert_eq!(log(&connection), None, "/clear starts a new log");
    }
}
