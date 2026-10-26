//! The project board (ADR 0118): typed append-only posts and fenced claims per project. Each rule
//! is a refusal here, so a skill only names the commands.

use super::*;
use std::ops::ControlFlow;

pub const KINDS: [&str; 6] = ["ask", "answer", "finding", "decision", "claim", "handoff"];
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

/// A stored board message that the caller rings after the commit.
#[derive(Debug, PartialEq)]
pub struct Ping {
    pub session_id: String,
    pub agent_id: String,
    pub seq: i64,
}

#[derive(Debug, PartialEq)]
pub enum Posted {
    New(i64, Vec<Ping>),
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
    #[serde(skip)]
    pub to_session: Option<String>,
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

pub(super) fn is_lead(
    tx: &Connection,
    session_id: &str,
    agent_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(orchestrator_of(tx, session_id).is_ok_and(|lead| lead == agent_id))
}

pub(super) fn kind_of(
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
        let found: Option<(i64, bool)> = tx
            .query_row(
                "SELECT n, session_id = ?3 AND author = ?4 AND kind = ?5
                 FROM post WHERE project_id = ?1 AND idem = ?2",
                (project_id, idem, session_id, author, new.kind),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match found {
            Some((n, true)) => return Ok(Posted::Again(n)),
            // Another seat's post, or another kind: a repeat would report a post this call did not make.
            Some((n, false)) => {
                return Err(
                    format!("swarm: --idem {idem} is used by B-{n}; pick another key").into(),
                );
            }
            None => {}
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

    let generation = match claim_fence(&tx, project_id, session_id, author, now, new)? {
        ControlFlow::Break(renewed) => {
            tx.commit()?;
            return Ok(renewed);
        }
        ControlFlow::Continue(generation) => generation,
    };

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
    let pings = board_pings(&tx, root, project_id, session_id, author, n, new, depth)?;
    let file = root.join(&path);
    std::fs::create_dir_all(file.parent().ok_or("body path has no parent")?)?;
    write_atomic(&file, new.body)?;
    tx.commit()?;
    Ok(Posted::New(n, pings))
}
