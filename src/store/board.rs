//! The project board (ADR 0115): typed append-only posts and fenced claims per project. Each rule
//! is a refusal here, so a skill only names the commands.

use super::*;

pub const KINDS: [&str; 6] = ["ask", "answer", "finding", "decision", "claim", "handoff"];
/// After this many holders, a work key parks for the owner instead of taking another claim.
const MAX_ATTEMPTS: i64 = 3;
const MAX_LEASE_SECS: i64 = 86_400;
const MAX_SUBJECT_CHARS: usize = 200;

#[derive(Default)]
pub struct NewPost<'a> {
    pub kind: &'a str,
    /// The recipient as (session, agent); the caller resolves a name to one live seat.
    pub to: Option<(&'a str, &'a str)>,
    pub reply_to: Option<i64>,
    pub supersedes: Option<i64>,
    pub work_key: Option<&'a str>,
    pub generation: Option<i64>,
    pub lease_secs: Option<i64>,
    pub needs_owner: bool,
    pub relayed: bool,
    pub sources: Vec<String>,
    pub subject: &'a str,
    pub body: &'a str,
    pub idem: Option<&'a str>,
}

#[derive(Debug, PartialEq)]
pub enum Posted {
    New(i64),
    /// The same `--idem` key was posted before; nothing was written.
    Again(i64),
    /// The holder renewed its lease; a renewal adds no post and no attempt.
    Renewed {
        generation: i64,
        lease_until: i64,
    },
}

#[derive(Debug, serde::Serialize)]
pub struct Post {
    pub n: i64,
    pub kind: String,
    pub session_id: String,
    pub author: String,
    pub author_role: String,
    pub to_agent: Option<String>,
    pub reply_to: Option<i64>,
    pub supersedes: Option<i64>,
    pub superseded_by: Option<i64>,
    pub work_key: Option<String>,
    pub generation: Option<i64>,
    pub depth: i64,
    pub needs_owner: bool,
    pub relayed: bool,
    pub sources: Vec<String>,
    pub subject: String,
    pub body_path: String,
    pub created_at: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct Claim {
    pub work_key: String,
    pub session_id: String,
    pub holder: String,
    pub generation: i64,
    pub attempts: i64,
    pub lease_until: i64,
    pub released_at: Option<i64>,
}

/// A source names what a post rests on: `path@blob` (a file and its git blob hash) or a URL.
pub fn valid_source(source: &str) -> bool {
    if source.chars().any(char::is_whitespace) {
        return false;
    }
    if source.starts_with("https://") || source.starts_with("http://") {
        return source.len() > "https://".len();
    }
    match source.rsplit_once('@') {
        Some((path, blob)) => {
            !path.is_empty()
                && (7..=64).contains(&blob.len())
                && blob.chars().all(|c| c.is_ascii_hexdigit())
        }
        None => false,
    }
}

/// Where a post's body lives under the swarm home. The project id is hashed, because it is a path.
pub fn body_path(project_id: &str, n: i64) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(project_id.as_bytes());
    let hex: String = hash[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("boards/{hex}/{n}.md")
}

fn is_lead(
    tx: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(orchestrator_of(tx, session_id).is_ok_and(|lead| lead == agent_id))
}

fn kind_of(
    tx: &Connection,
    project_id: &str,
    n: i64,
) -> Result<String, Box<dyn std::error::Error>> {
    tx.query_row(
        "SELECT kind FROM post WHERE project_id = ?1 AND n = ?2",
        (project_id, n),
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| format!("swarm: no post B-{n} on this board").into())
}

#[allow(clippy::too_many_arguments)]
pub fn post(
    connection: &mut Connection,
    root: &Path,
    project_id: &str,
    session_id: &str,
    author: &str,
    now: i64,
    new: &NewPost,
) -> Result<Posted, Box<dyn std::error::Error>> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(idem) = new.idem {
        let found: Option<i64> = tx
            .query_row(
                "SELECT n FROM post WHERE project_id = ?1 AND idem = ?2",
                (project_id, idem),
                |r| r.get(0),
            )
            .optional()?;
        if let Some(n) = found {
            return Ok(Posted::Again(n));
        }
    }
    if !KINDS.contains(&new.kind) {
        return Err(format!(
            "swarm: unknown kind {:?}; use one of {}",
            new.kind,
            KINDS.join(", ")
        )
        .into());
    }
    let subject = new.subject.trim();
    if subject.is_empty()
        || subject.chars().count() > MAX_SUBJECT_CHARS
        || subject.chars().any(char::is_control)
    {
        return Err(format!(
            "swarm: needs --subject, one line of 1 to {MAX_SUBJECT_CHARS} characters"
        )
        .into());
    }
    let role: String = tx
        .query_row(
            "SELECT role FROM agent WHERE session_id = ?1 AND id = ?2",
            (session_id, author),
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| format!("swarm: {author} is not an agent of session {session_id}"))?;
    let lead = is_lead(&tx, session_id, author)?;

    if matches!(new.kind, "finding" | "decision" | "answer") && new.sources.is_empty() {
        return Err("swarm: needs --source path@blob or URL".into());
    }
    if let Some(bad) = new.sources.iter().find(|s| !valid_source(s)) {
        return Err(format!("swarm: needs --source path@blob or URL, not {bad:?}").into());
    }
    if new.relayed && !lead {
        return Err("swarm: only the lead relays the owner".into());
    }
    if new.relayed && !matches!(new.kind, "answer" | "decision") {
        return Err("swarm: --relayed fits only an answer or a decision".into());
    }
    if new.kind == "ask" && new.to.is_none() && !lead {
        return Err("swarm: only the lead asks everyone; name --to".into());
    }
    if new.kind == "decision" && !lead && !new.needs_owner {
        return Err("swarm: ask the lead, or add --needs-owner".into());
    }
    if new.needs_owner && new.kind != "decision" {
        return Err("swarm: --needs-owner fits only a decision".into());
    }
    let mut depth = 0;
    if let Some(parent) = new.reply_to {
        let parent_kind = kind_of(&tx, project_id, parent)?;
        if new.kind == "answer" && parent_kind != "ask" {
            return Err(format!("swarm: B-{parent} is a {parent_kind}, not an ask").into());
        }
        let parent_depth: i64 = tx.query_row(
            "SELECT depth FROM post WHERE project_id = ?1 AND n = ?2",
            (project_id, parent),
            |r| r.get(0),
        )?;
        depth = parent_depth + i64::from(new.kind == "ask");
    } else if new.kind == "answer" {
        return Err("swarm: an answer names the ask it answers".into());
    }
    if let Some(old) = new.supersedes {
        let old_kind = kind_of(&tx, project_id, old)?;
        if old_kind != new.kind {
            return Err(format!(
                "swarm: B-{old} is a {old_kind}; a {} cannot supersede it",
                new.kind
            )
            .into());
        }
        let by: Option<i64> = tx
            .query_row(
                "SELECT n FROM post WHERE project_id = ?1 AND supersedes = ?2",
                (project_id, old),
                |r| r.get(0),
            )
            .optional()?;
        if let Some(by) = by {
            return Err(format!("swarm: B-{old} is superseded by B-{by}; supersede B-{by}").into());
        }
    }

    let mut generation = None;
    match new.kind {
        "claim" => {
            let work = new.work_key.ok_or("swarm: a claim needs --work <key>")?;
            let lease = new
                .lease_secs
                .ok_or("swarm: a claim needs --lease <seconds>")?;
            if !(1..=MAX_LEASE_SECS).contains(&lease) {
                return Err(format!("swarm: --lease is 1 to {MAX_LEASE_SECS} seconds").into());
            }
            let current = claim_row(&tx, project_id, work)?;
            match current {
                None => {
                    tx.execute(
                        "INSERT INTO claim (project_id, work_key, session_id, holder, generation, attempts, lease_until)
                         VALUES (?1, ?2, ?3, ?4, 1, 1, ?5)",
                        (project_id, work, session_id, author, now + lease),
                    )?;
                    generation = Some(1);
                }
                Some(c) => {
                    let holds =
                        c.session_id == session_id && c.holder == author && c.released_at.is_none();
                    if holds {
                        return match new.generation {
                            Some(g) if g == c.generation => {
                                tx.execute(
                                    "UPDATE claim SET lease_until = ?3 WHERE project_id = ?1 AND work_key = ?2",
                                    (project_id, work, now + lease),
                                )?;
                                tx.commit()?;
                                Ok(Posted::Renewed {
                                    generation: g,
                                    lease_until: now + lease,
                                })
                            }
                            Some(g) => Err(format!(
                                "swarm: stale generation {g}; current is {}",
                                c.generation
                            )
                            .into()),
                            None => Err(format!(
                                "swarm: you hold {work}, generation {}; renew with --generation {}",
                                c.generation, c.generation
                            )
                            .into()),
                        };
                    }
                    if c.released_at.is_none() && c.lease_until > now {
                        return Err(format!(
                            "swarm: held by {} for {} s more, generation {}",
                            c.holder,
                            c.lease_until - now,
                            c.generation
                        )
                        .into());
                    }
                    if c.attempts >= MAX_ATTEMPTS {
                        return Err(format!(
                            "swarm: {MAX_ATTEMPTS} attempts used on {work}; parked for owner"
                        )
                        .into());
                    }
                    tx.execute(
                        "UPDATE claim SET session_id = ?3, holder = ?4, generation = generation + 1,
                                attempts = attempts + 1, lease_until = ?5, released_at = NULL
                         WHERE project_id = ?1 AND work_key = ?2",
                        (project_id, work, session_id, author, now + lease),
                    )?;
                    generation = Some(c.generation + 1);
                }
            }
        }
        "handoff" => {
            let work = new.work_key.ok_or("swarm: a handoff needs --work <key>")?;
            let g = new
                .generation
                .ok_or("swarm: a handoff needs --generation <n> from your claim")?;
            let c = claim_row(&tx, project_id, work)?
                .ok_or_else(|| format!("swarm: nobody claimed {work}"))?;
            if c.generation != g {
                return Err(
                    format!("swarm: stale generation {g}; current is {}", c.generation).into(),
                );
            }
            if c.session_id != session_id || c.holder != author || c.released_at.is_some() {
                return Err(format!("swarm: you do not hold {work}").into());
            }
            tx.execute(
                "UPDATE claim SET released_at = ?3 WHERE project_id = ?1 AND work_key = ?2",
                (project_id, work, now),
            )?;
            generation = Some(g);
        }
        _ => {}
    }

    let n: i64 = tx.query_row(
        "SELECT COALESCE(MAX(n), 0) + 1 FROM post WHERE project_id = ?1",
        [project_id],
        |r| r.get(0),
    )?;
    let path = body_path(project_id, n);
    tx.execute(
        "INSERT INTO post (project_id, n, kind, session_id, author, author_role, to_session, to_agent,
                           reply_to, supersedes, work_key, generation, depth, needs_owner, relayed,
                           sources, subject, body_path, idem, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        rusqlite::params![
            project_id,
            n,
            new.kind,
            session_id,
            author,
            role,
            new.to.map(|t| t.0),
            new.to.map(|t| t.1),
            new.reply_to,
            new.supersedes,
            new.work_key,
            generation,
            depth,
            new.needs_owner,
            new.relayed,
            serde_json::to_string(&new.sources)?,
            subject,
            path,
            new.idem,
            now,
        ],
    )?;
    let file = root.join(&path);
    std::fs::create_dir_all(file.parent().ok_or("body path has no parent")?)?;
    write_atomic(&file, new.body)?;
    tx.commit()?;
    Ok(Posted::New(n))
}

fn claim_row(tx: &Connection, project_id: &str, work: &str) -> rusqlite::Result<Option<Claim>> {
    tx.query_row(
        "SELECT work_key, session_id, holder, generation, attempts, lease_until, released_at
         FROM claim WHERE project_id = ?1 AND work_key = ?2",
        (project_id, work),
        claim_from,
    )
    .optional()
}

fn claim_from(r: &rusqlite::Row) -> rusqlite::Result<Claim> {
    Ok(Claim {
        work_key: r.get(0)?,
        session_id: r.get(1)?,
        holder: r.get(2)?,
        generation: r.get(3)?,
        attempts: r.get(4)?,
        lease_until: r.get(5)?,
        released_at: r.get(6)?,
    })
}

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

pub fn claims(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<Claim>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT work_key, session_id, holder, generation, attempts, lease_until, released_at
         FROM claim WHERE project_id = ?1 ORDER BY work_key",
    )?;
    let rows = statement.query_map([project_id], claim_from)?;
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
