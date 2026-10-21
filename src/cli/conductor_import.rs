//! `swarm import conductor` (ADR 0106): Conductor's chats and tabs become ended swarm chats.

use super::*;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use swarm::managed::{Edit, Kind, Writer};

mod sessions;
use sessions::*;

/// One Conductor session, as its database stores it.
#[derive(Clone, Debug, Default)]
struct Session {
    id: String,
    agent_type: String,
    provider_id: Option<String>,
    title: String,
    hidden: bool,
    created_at: i64,
    updated_at: i64,
    cwd: Option<String>,
    selected: bool,
    messages: i64,
    last_message_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Chat {
    conductor_id: String,
    /// A UUID v7 at the Conductor creation time, so a continuation's newer id sorts after it.
    id: String,
    /// The CLI that ran the chat; its log is in this CLI's format.
    provider: String,
    /// The id the CLI resumes: its own, or the swarm id of a session swarm rebuilds.
    provider_id: Option<String>,
    title: String,
    cwd: String,
    created_at: i64,
    active_at: i64,
    open: bool,
    selected: bool,
    closed_at: Option<i64>,
    messages: i64,
    log: String,
    /// False when swarm rebuilds `log` from Conductor's message rows, in the CLI's own folder.
    native_log: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Skip {
    conductor_id: String,
    title: String,
    reason: String,
}

#[derive(Debug, Default, Serialize)]
struct Plan {
    chats: Vec<Chat>,
    skipped: Vec<Skip>,
}

impl Plan {
    fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("a plan serializes");
        format!("{:x}", Sha256::digest(bytes))
    }
}

/// What swarm already has, so an import adds each chat once.
#[derive(Default)]
struct Known {
    provider_ids: BTreeSet<String>,
    logs: BTreeSet<String>,
}

/// `swarm import conductor [--json | --digest <digest>] [--db <path>]`. With no digest it prints
/// the plan and writes nothing; the digest of that plan is the owner's consent to apply it.
pub(crate) fn import(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let usage = "usage: swarm import conductor [--json | --digest <digest>] [--db <path>]";
    let [source, flags @ ..] = args else {
        return Err(usage.into());
    };
    if source != "conductor" {
        return Err(usage.into());
    }
    let (mut json, mut digest, mut db) = (false, None, None);
    let mut rest = flags.iter();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--json" => json = true,
            "--digest" => digest = Some(rest.next().ok_or(usage)?.as_str()),
            "--db" => db = Some(PathBuf::from(rest.next().ok_or(usage)?)),
            _ => return Err(usage.into()),
        }
    }
    if json && digest.is_some() {
        return Err(usage.into());
    }
    let db = match db {
        Some(db) => db,
        None => PathBuf::from(env_var("HOME")?)
            .join("Library/Application Support/com.conductor.app/conductor.db"),
    };
    if !db.is_file() {
        return Err(format!("swarm: no Conductor database at {}", db.display()).into());
    }
    let root = swarm::paths::root_dir()?;
    let copy = Copy::of(&db)?;
    let conductor = Connection::open(&copy.db)?;
    let mut store = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let (claude_projects, codex_sessions) = (
        claude_config()?.join("projects"),
        default_codex_home()?.join("sessions"),
    );
    let plan = plan(
        read_sessions(&conductor)?,
        &known(&store)?,
        |session| native_log(&conductor, session),
        |session, id| match session.agent_type.as_str() {
            "codex" => codex_sessions.join(rollout_name(&conductor, session.created_at, id)),
            _ => claude_projects
                .join(claude_project(Path::new(
                    session.cwd.as_deref().unwrap_or_default(),
                )))
                .join(format!("{id}.jsonl")),
        },
    );
    let Some(digest) = digest else {
        if json {
            return print_json(&json!({
                "digest": plan.digest(),
                "source": db.to_string_lossy(),
                "chats": plan.chats,
                "skipped": plan.skipped,
            }));
        }
        print!("{}", plan_text(&plan));
        if !plan.chats.is_empty() {
            println!(
                "Apply: swarm import conductor --digest {}{}",
                plan.digest(),
                if args.iter().any(|a| a == "--db") {
                    format!(
                        " --db {}",
                        swarm::adapter::shell_line(&[db.to_string_lossy().into()])
                    )
                } else {
                    String::new()
                }
            );
        }
        return Ok(());
    };
    if let Some(reason) = swarm::bus::setup_refusal(
        env::var("SWARM_AGENT_ID").ok().as_deref(),
        env::var("HERDR_AGENT_PANE").ok().as_deref(),
    ) {
        return Err(reason.into());
    }
    if plan.digest() != digest {
        return Err(
            "swarm: Conductor or swarm changed after the plan; check the plan again".into(),
        );
    }
    // The lock that every write outside the swarm home takes (ADR 0042).
    swarm::managed::with_lock(&swarm::paths::trust_lock()?, || {
        apply(&root, &conductor, &mut store, &plan).map_err(|error| error.to_string())
    })?;
    let open = plan.chats.iter().filter(|c| c.open).count();
    println!(
        "swarm: imported {} chats from Conductor, {open} as open tabs and {} in Recently closed",
        plan.chats.len(),
        plan.chats.len() - open
    );
    Ok(())
}

/// A copy of Conductor's database with its write-ahead log, so a read never touches the app's
/// file and a schema error leaves nothing behind.
struct Copy {
    dir: PathBuf,
    db: PathBuf,
}

impl Copy {
    fn of(db: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = env::temp_dir().join(format!("swarm-conductor-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir)?;
        let copy = Self {
            db: dir.join("conductor.db"),
            dir,
        };
        std::fs::copy(db, &copy.db)?;
        for suffix in ["-wal", "-shm"] {
            let from = PathBuf::from(format!("{}{suffix}", db.display()));
            if from.is_file() {
                std::fs::copy(&from, format!("{}{suffix}", copy.db.display()))?;
            }
        }
        Ok(copy)
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn schema_error(error: rusqlite::Error) -> String {
    format!("swarm: this Conductor database has a layout the import does not know ({error})")
}

fn read_sessions(conductor: &Connection) -> Result<Vec<Session>, String> {
    let mut statement = conductor
        .prepare(
            "SELECT s.id, s.agent_type, nullif(s.claude_session_id, ''), coalesce(s.title, ''),
                    coalesce(s.is_hidden, 0), unixepoch(s.created_at),
                    coalesce(unixepoch(s.updated_at), unixepoch(s.created_at)),
                    nullif(w.workspace_path, ''), w.active_session_id IS s.id,
                    (SELECT count(*) FROM session_messages m
                     WHERE m.session_id = s.id AND m.cancelled_at IS NULL),
                    (SELECT max(unixepoch(m.created_at)) FROM session_messages m
                     WHERE m.session_id = s.id)
             FROM sessions s LEFT JOIN workspaces w ON w.id = s.workspace_id
             ORDER BY s.created_at, s.id",
        )
        .map_err(schema_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(Session {
                id: row.get(0)?,
                agent_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                provider_id: row.get(2)?,
                title: row.get(3)?,
                hidden: row.get::<_, i64>(4)? != 0,
                created_at: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                updated_at: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                cwd: row.get(7)?,
                selected: row.get(8)?,
                messages: row.get(9)?,
                last_message_at: row.get(10)?,
            })
        })
        .map_err(schema_error)?;
    rows.collect::<Result<_, _>>().map_err(schema_error)
}

fn known(store: &Connection) -> Result<Known, rusqlite::Error> {
    let mut known = Known::default();
    let mut statement = store.prepare("SELECT chair_id, chair_log FROM session")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        known.provider_ids.extend(row.get::<_, Option<String>>(0)?);
        known.logs.extend(row.get::<_, Option<String>>(1)?);
    }
    Ok(known)
}

/// The provider's own log of `session`, where the provider keeps it on this Mac.
fn native_log(conductor: &Connection, session: &Session) -> Option<PathBuf> {
    let id = session.provider_id.as_deref()?;
    match session.agent_type.as_str() {
        "claude" => claude_chair_log(id),
        "codex" => {
            let days = conductor
                .query_row(
                    "SELECT strftime('%Y/%m/%d', ?1 - 86400, 'unixepoch'),
                            strftime('%Y/%m/%d', ?1, 'unixepoch'),
                            strftime('%Y/%m/%d', ?1 + 86400, 'unixepoch')",
                    [session.created_at],
                    |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]),
                )
                .ok()?;
            codex_chair_log(id, &days)
        }
        _ => None,
    }
}

/// Which Conductor sessions become swarm chats, and why each other one is skipped.
fn plan(
    sessions: Vec<Session>,
    known: &Known,
    native_log: impl Fn(&Session) -> Option<PathBuf>,
    rebuilt_log: impl Fn(&Session, &str) -> PathBuf,
) -> Plan {
    // Of two sessions with one provider id, the one with more messages, then the older one.
    let mut best: BTreeMap<&str, &Session> = BTreeMap::new();
    for session in &sessions {
        if let Some(id) = session.provider_id.as_deref() {
            let kept = best.entry(id).or_insert(session);
            if session.messages > kept.messages {
                *kept = session;
            }
        }
    }
    let mut result = Plan::default();
    for session in &sessions {
        let skip = |reason: String| Skip {
            conductor_id: session.id.clone(),
            title: session.title.clone(),
            reason,
        };
        if !["claude", "codex"].contains(&session.agent_type.as_str()) {
            let reason = format!("a {} chat has no log swarm reads", session.agent_type);
            result.skipped.push(skip(reason));
            continue;
        }
        if session.messages == 0 {
            result.skipped.push(skip("no messages".into()));
            continue;
        }
        let Some(cwd) = session.cwd.clone() else {
            result.skipped.push(skip("no workspace folder".into()));
            continue;
        };
        if let Some(id) = session.provider_id.as_deref() {
            let kept = best[id];
            if kept.id != session.id {
                let reason = format!("the same chat as Conductor session {}", kept.id);
                result.skipped.push(skip(reason));
                continue;
            }
            if known.provider_ids.contains(id) {
                result.skipped.push(skip("already in swarm".into()));
                continue;
            }
        }
        let id = swarm_id(session);
        let native = native_log(session);
        // A rebuilt session takes the swarm id, so its file name is known before it is written.
        let (log, provider_id) = match &native {
            Some(log) => (log.clone(), session.provider_id.clone()),
            None => (rebuilt_log(session, &id), Some(id.clone())),
        };
        let log = log.to_string_lossy().into_owned();
        if known.logs.contains(&log)
            || provider_id
                .as_ref()
                .is_some_and(|id| known.provider_ids.contains(id))
        {
            result.skipped.push(skip("already in swarm".into()));
            continue;
        }
        result.chats.push(Chat {
            conductor_id: session.id.clone(),
            id,
            provider: session.agent_type.clone(),
            provider_id,
            title: if session.title == "Untitled" {
                String::new()
            } else {
                session.title.clone()
            },
            cwd,
            created_at: session.created_at,
            active_at: session.last_message_at.unwrap_or(session.updated_at),
            open: !session.hidden,
            selected: session.selected && !session.hidden,
            closed_at: session.hidden.then_some(session.updated_at),
            messages: session.messages,
            log,
            native_log: native.is_some(),
        });
    }
    result
}

/// A UUID v7 at the session's creation time whose other bytes come from its Conductor id, so a
/// plan and its apply name the same chat.
fn swarm_id(session: &Session) -> String {
    let hash = Sha256::digest(session.id.as_bytes());
    let bytes: [u8; 10] = hash[..10].try_into().unwrap();
    let millis = u64::try_from(session.created_at).unwrap_or_default() * 1000;
    uuid::Builder::from_unix_timestamp_millis(millis, &bytes)
        .into_uuid()
        .to_string()
}

fn plan_text(plan: &Plan) -> String {
    let mut text = String::new();
    if plan.chats.is_empty() {
        text.push_str("swarm: nothing to import from Conductor.\n");
    }
    for chat in &plan.chats {
        text.push_str(&format!(
            "import  {:<6} {:<6} {} · {} messages · {}{}\n",
            chat.provider,
            if chat.open { "open" } else { "closed" },
            if chat.title.is_empty() {
                &chat.conductor_id
            } else {
                &chat.title
            },
            chat.messages,
            chat.cwd,
            if chat.native_log {
                ""
            } else {
                " · session rebuilt from Conductor"
            },
        ));
    }
    for skip in &plan.skipped {
        text.push_str(&format!(
            "skip    {} {}: {}\n",
            skip.conductor_id, skip.title, skip.reason
        ));
    }
    text
}

/// Writes each rebuilt session through `managed` (ADR 0042), under the caller's trust lock, then
/// the chats in one transaction, then the owner's tabs. The database comes before the choices, as
/// the app drops an imported key whose chat it cannot find.
fn apply(
    root: &Path,
    conductor: &Connection,
    store: &mut Connection,
    plan: &Plan,
) -> Result<(), Box<dyn std::error::Error>> {
    let rebuilt: Vec<&Chat> = plan.chats.iter().filter(|chat| !chat.native_log).collect();
    let mut plans = Vec::new();
    for chat in &rebuilt {
        let id = chat.provider_id.as_deref().unwrap_or(&chat.id);
        let records = records(conductor, chat)?;
        let text = match chat.provider.as_str() {
            "codex" => codex_log(records, &chat.cwd, id),
            _ => claude_log(records, &chat.cwd, id),
        };
        let path = PathBuf::from(&chat.log);
        let edit = Edit::new(
            Writer::ImportConductor,
            &path,
            Kind::File,
            &[],
            swarm::managed::file_hash(&text),
        );
        plans.push(swarm::managed::Plan::from(swarm::managed::FilePlan {
            path,
            before: String::new(),
            after: text,
            conflicts: Vec::new(),
            edits: vec![edit],
        }));
    }
    swarm::managed::apply(store, &plans)?;
    for chat in &rebuilt {
        // The sidebar and each CLI's picker sort by the file's time: the last message, not now.
        let at = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(u64::try_from(chat.active_at).unwrap_or_default());
        std::fs::File::options()
            .write(true)
            .open(&chat.log)?
            .set_modified(at)?;
    }
    let tx = store.transaction()?;
    for chat in &plan.chats {
        tx.execute(
            // No adapter: the chat has no pane to list, and the owner's `imported` set lists it.
            "INSERT INTO session (id, talk_mode, cwd, created_at, chair_log,
                                  chair_provider, chair_id, archived_at)
             VALUES (?1, 'lane', ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                &chat.id,
                &chat.cwd,
                chat.created_at,
                &chat.log,
                &chat.provider,
                &chat.provider_id,
                chat.closed_at,
            ),
        )?;
        // The chair ended with Conductor, so its row has no pane.
        tx.execute(
            "INSERT INTO agent (id, session_id, role, provider, created_at, log)
             VALUES ('orchestrator', ?1, 'orchestrator', ?2, ?3, ?4)",
            (&chat.id, &chat.provider, chat.created_at, &chat.log),
        )?;
    }
    tx.commit()?;
    swarm::view::choices_store::update(root, None, |owner, state| {
        for chat in &plan.chats {
            if !chat.title.is_empty() {
                owner
                    .chat_names
                    .entry(chat.id.clone())
                    .or_insert_with(|| chat.title.clone());
            }
            if !chat.open {
                continue;
            }
            owner.imported.insert(chat.id.clone());
            // A removed worktree has no tab strip; its chats show under "Removed worktrees".
            if !Path::new(&chat.cwd).is_dir() {
                continue;
            }
            let strip = owner.tabs.entry(chat.cwd.clone()).or_default();
            if !strip.open.contains(&chat.id) {
                strip.open.push(chat.id.clone());
            }
            if chat.selected {
                state
                    .selected_chats
                    .entry(chat.cwd.clone())
                    .or_insert_with(|| chat.id.clone());
            }
        }
        Ok(())
    })
    .map_err(|error| {
        format!("swarm: the chats were added, but their tabs were not saved ({error})")
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "conductor_import/tests.rs"]
mod tests;
