//! Message rings, delivery results, and stalls.

use super::*;

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

/// Whether a hook or a host record of `agent_id` reported a turn at or after `since` (unix
/// seconds). `done` is no turn start, because Claude's idle notice writes it on an idle pane.
pub fn turn_started(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    since: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let started: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM agent
                        WHERE session_id = ?1 AND id = ?2 AND state_source IN ('hook', 'host')
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
