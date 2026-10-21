//! `swarm import conductor` (ADR 0106): Conductor's chats and tabs become ended swarm chats.

use super::*;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

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
    /// The CLI that ran the chat, for the chair agent row.
    provider: String,
    /// The log format, which names the chat's provider in swarm: a converted log is Claude's.
    format: String,
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
    /// False when swarm writes `log` from Conductor's message rows.
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
    let plan = plan(
        read_sessions(&conductor)?,
        &known(&store)?,
        &root.join("imports/conductor"),
        |session| native_log(&conductor, session),
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
    apply(&root, &conductor, &mut store, &plan)?;
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
    converted_dir: &Path,
    native_log: impl Fn(&Session) -> Option<PathBuf>,
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
        let native = native_log(session);
        let log = native
            .clone()
            .unwrap_or_else(|| converted_dir.join(format!("{}.jsonl", session.id)));
        let log = log.to_string_lossy().into_owned();
        if known.logs.contains(&log) {
            result.skipped.push(skip("already in swarm".into()));
            continue;
        }
        result.chats.push(Chat {
            conductor_id: session.id.clone(),
            id: swarm_id(session),
            provider: session.agent_type.clone(),
            format: if native.is_some() {
                session.agent_type.clone()
            } else {
                "claude".into()
            },
            provider_id: session.provider_id.clone(),
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
                " · log copied from Conductor"
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

/// One line of a Claude-format log from one Conductor message row, or None for a row that has no
/// place in the log: a status event, or a step of a subagent, which Claude keeps in another file.
fn log_line(role: &str, content: &str, id: &str, at: &str, session: &str) -> Option<Value> {
    if role == "user" {
        return Some(json!({
            "type": "user",
            "sessionId": session,
            "uuid": id,
            "timestamp": at,
            "message": {"role": "user", "content": content},
        }));
    }
    let mut event: Value = serde_json::from_str(content).ok()?;
    let record = event.as_object_mut()?;
    if !matches!(record.get("type")?.as_str()?, "assistant" | "user")
        || record
            .get("parent_tool_use_id")
            .is_some_and(|parent| !parent.is_null())
    {
        return None;
    }
    let session = record
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or(session)
        .to_owned();
    record.entry("sessionId").or_insert(session.into());
    record.entry("uuid").or_insert(id.into());
    record.entry("timestamp").or_insert(at.into());
    if let Some(result) = record.remove("tool_use_result") {
        record.entry("toolUseResult").or_insert(result);
    }
    Some(event)
}

fn converted_log(conductor: &Connection, chat: &Chat) -> Result<String, String> {
    let mut statement = conductor
        .prepare(
            "SELECT id, coalesce(role, ''), coalesce(content, ''), created_at
             FROM session_messages WHERE session_id = ?1 AND cancelled_at IS NULL
             ORDER BY created_at, rowid",
        )
        .map_err(schema_error)?;
    let session = chat.provider_id.as_deref().unwrap_or(&chat.conductor_id);
    let mut text = String::new();
    let mut rows = statement
        .query([&chat.conductor_id])
        .map_err(schema_error)?;
    while let Some(row) = rows.next().map_err(schema_error)? {
        let get = |i| row.get::<_, String>(i).map_err(schema_error);
        if let Some(line) = log_line(&get(1)?, &get(2)?, &get(0)?, &get(3)?, session) {
            text.push_str(&line.to_string());
            text.push('\n');
        }
    }
    Ok(text)
}

/// Writes each converted log, then the chats in one transaction, then the owner's tabs. The
/// database comes before the choices, as the app drops an imported key whose chat it cannot find.
fn apply(
    root: &Path,
    conductor: &Connection,
    store: &mut Connection,
    plan: &Plan,
) -> Result<(), Box<dyn std::error::Error>> {
    for chat in plan.chats.iter().filter(|chat| !chat.native_log) {
        let path = Path::new(&chat.log);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let partial = path.with_extension(format!("jsonl.{}", uuid::Uuid::now_v7()));
        std::fs::write(&partial, converted_log(conductor, chat)?)?;
        // The sidebar sorts a chat by its log's time, which is the last message, not this import.
        let at = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(u64::try_from(chat.active_at).unwrap_or_default());
        std::fs::File::options()
            .write(true)
            .open(&partial)?
            .set_modified(at)?;
        std::fs::rename(&partial, path)?;
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
                &chat.format,
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
