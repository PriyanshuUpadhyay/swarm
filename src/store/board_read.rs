//! Board reads and relink (ADR 0124).

use super::*;

/// The role whose posts a worker does not read: the checker's tests stay out of the coder's view.
pub const CHECKER_ROLE: &str = "checker";

/// The posts that `reader` (session, agent) may read; None is the owner at a plain terminal. A
/// worker does not get the posts of its own session's checker, except one sent to it and an
/// answer to its own ask, so a ping never points at a post it cannot read. A checker post from
/// another session is a finished task's fact and stays readable. Best effort: the body files stay
/// readable.
pub fn posts_for(
    connection: &Connection,
    project_id: &str,
    reader: Option<(&str, &str)>,
) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
    let mut all = posts(connection, project_id)?;
    let Some((session, agent)) = reader else {
        return Ok(all);
    };
    if is_checker(connection, session, agent)? || is_lead(connection, session, agent)? {
        return Ok(all);
    }
    let mine = |p: &Post| p.session_id == session && p.author == agent;
    let asked: Vec<i64> = all
        .iter()
        .filter(|p| p.kind == "ask" && mine(p))
        .map(|p| p.n)
        .collect();
    all.retain(|p| {
        p.author_role != CHECKER_ROLE
            || p.session_id != session
            || (p.to_session.as_deref() == Some(session) && p.to_agent.as_deref() == Some(agent))
            || (p.kind == "answer" && p.reply_to.is_some_and(|n| asked.contains(&n)))
    });
    Ok(all)
}

/// A checker reads only the posts its brief names, so a list of the board, which shows every
/// subject, is refused to it. `swarm board read B-n` still works. The rule is here and not in the
/// checker's skill, because an old installed copy of that skill tells every seat to list the board.
pub fn may_list(
    connection: &Connection,
    reader: Option<(&str, &str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    match reader {
        Some((session, agent)) if is_checker(connection, session, agent)? => Err(
            "swarm: a checker reads only the posts its brief names; run `swarm board read B-n` for each one"
                .into(),
        ),
        _ => Ok(()),
    }
}

fn is_checker(connection: &Connection, session: &str, agent: &str) -> rusqlite::Result<bool> {
    let role: Option<String> = connection
        .query_row(
            "SELECT role FROM agent WHERE session_id = ?1 AND id = ?2",
            (session, agent),
            |r| r.get(0),
        )
        .optional()?;
    Ok(role.as_deref() == Some(CHECKER_ROLE))
}

pub fn posts(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT p.n, p.kind, p.session_id, p.author, p.author_role, p.to_agent, p.reply_to, p.supersedes,
                p.to_session,
                (SELECT s.n FROM post s WHERE s.project_id = p.project_id AND s.supersedes = p.n),
                p.work_key, p.generation, p.depth, p.needs_owner, p.relayed, p.sources, p.subject,
                p.body_path, p.created_at
         FROM post p WHERE p.project_id = ?1 ORDER BY p.n",
    )?;
    let rows = statement.query_map([project_id], |r| {
        let sources: String = r.get(15)?;
        Ok(Post {
            n: r.get(0)?,
            kind: r.get(1)?,
            session_id: r.get(2)?,
            author: r.get(3)?,
            author_role: r.get(4)?,
            to_agent: r.get(5)?,
            reply_to: r.get(6)?,
            supersedes: r.get(7)?,
            to_session: r.get(8)?,
            superseded_by: r.get(9)?,
            work_key: r.get(10)?,
            generation: r.get(11)?,
            depth: r.get(12)?,
            needs_owner: r.get(13)?,
            relayed: r.get(14)?,
            sources: serde_json::from_str(&sources).unwrap_or_default(),
            subject: r.get(16)?,
            body_path: r.get(17)?,
            created_at: r.get(18)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Whether `agent` wrote any post in `session`.
pub fn has_posted(connection: &Connection, session: &str, agent: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM post WHERE session_id = ?1 AND author = ?2)",
        [session, agent],
        |r| r.get(0),
    )
}

/// Whether `agent` acked any message in `session`, so its first `inbox` is behind it.
pub fn has_acked(connection: &Connection, session: &str, agent: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM read_mark WHERE session_id = ?1 AND agent_id = ?2)",
        [session, agent],
        |r| r.get(0),
    )
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
