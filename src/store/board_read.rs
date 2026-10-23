//! Board reads and relink (ADR 0115).

use super::*;

pub fn posts(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT p.n, p.kind, p.session_id, p.author, p.author_role, p.to_agent, p.reply_to, p.supersedes,
                (SELECT s.n FROM post s WHERE s.project_id = p.project_id AND s.supersedes = p.n),
                p.work_key, p.generation, p.depth, p.needs_owner, p.relayed, p.sources, p.subject,
                p.body_path, p.created_at
         FROM post p WHERE p.project_id = ?1 ORDER BY p.n",
    )?;
    let rows = statement.query_map([project_id], |r| {
        let sources: String = r.get(14)?;
        Ok(Post {
            n: r.get(0)?,
            kind: r.get(1)?,
            session_id: r.get(2)?,
            author: r.get(3)?,
            author_role: r.get(4)?,
            to_agent: r.get(5)?,
            reply_to: r.get(6)?,
            supersedes: r.get(7)?,
            superseded_by: r.get(8)?,
            work_key: r.get(9)?,
            generation: r.get(10)?,
            depth: r.get(11)?,
            needs_owner: r.get(12)?,
            relayed: r.get(13)?,
            sources: serde_json::from_str(&sources).unwrap_or_default(),
            subject: r.get(15)?,
            body_path: r.get(16)?,
            created_at: r.get(17)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Copies a moved repository's board to its new id. The old rows stay, because posts are
/// append-only. Only an empty board takes a copy, so post numbers and links keep their meaning.
pub fn relink(
    connection: &mut Connection,
    from: &str,
    to: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let count = |id: &str| -> rusqlite::Result<i64> {
        tx.query_row(
            "SELECT count(*) FROM post WHERE project_id = ?1",
            [id],
            |r| r.get(0),
        )
    };
    if from == to {
        return Err("swarm: the board is already at this id".into());
    }
    if count(from)? == 0 {
        return Err(format!("swarm: no board at {from}").into());
    }
    if count(to)? > 0 {
        return Err(format!(
            "swarm: board {to} already has posts; relink copies only into an empty board"
        )
        .into());
    }
    let copied = tx.execute(
        "INSERT INTO post (project_id, n, kind, session_id, author, author_role, to_session, to_agent,
                           reply_to, supersedes, work_key, generation, depth, needs_owner, relayed,
                           sources, subject, body_path, idem, created_at)
         SELECT ?2, n, kind, session_id, author, author_role, to_session, to_agent, reply_to,
                supersedes, work_key, generation, depth, needs_owner, relayed, sources, subject,
                body_path, idem, created_at
         FROM post WHERE project_id = ?1 ORDER BY n",
        (from, to),
    )?;
    tx.execute(
        "INSERT INTO claim (project_id, work_key, session_id, holder, generation, attempts, lease_until, released_at)
         SELECT ?2, work_key, session_id, holder, generation, attempts, lease_until, released_at
         FROM claim WHERE project_id = ?1",
        (from, to),
    )?;
    tx.commit()?;
    Ok(copied)
}
