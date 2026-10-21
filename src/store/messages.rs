//! Message storage, routing, and read marks.

use super::*;

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

/// Whether `sender_id` sent a summary after the last message it got, so a child that answered one
/// ask and then died on the next one still counts as unanswered.
pub fn answered(
    connection: &Connection,
    session_id: &str,
    sender_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM message
             WHERE session_id = ?1 AND sender_id = ?2 AND kind = 'summary'
               AND seq > (SELECT COALESCE(MAX(seq), -1) FROM message
                          WHERE session_id = ?1 AND recipient_id = ?2))",
        (session_id, sender_id),
        |r| r.get(0),
    )?)
}

/// Whether `agent` got a wrap-up after its last summary.
pub fn owes_wrap_up(
    connection: &Connection,
    session_id: &str,
    agent: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM message
             WHERE session_id = ?1 AND recipient_id = ?2 AND kind = 'wrap-up'
               AND seq > (SELECT COALESCE(MAX(seq), -1) FROM message
                          WHERE session_id = ?1 AND sender_id = ?2 AND kind = 'summary'))",
        (session_id, agent),
        |r| r.get(0),
    )?)
}

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
        return Err(format!("swarm: no message {seq} in your inbox").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

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
    fn ack_of_a_seq_not_in_the_inbox_names_the_seq() {
        let connection = seed(0);
        let error = ack(&connection, SESSION, 999999, CODER).unwrap_err();
        assert_eq!(error.to_string(), "swarm: no message 999999 in your inbox");
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
}
