use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

mod agent_state;
mod agents;
mod delivery;
mod messages;
mod sessions;
mod usage;
pub use agent_state::*;
pub use agents::*;
pub use delivery::*;
pub use messages::*;
pub use sessions::*;
pub use usage::*;

#[cfg(test)]
#[path = "store/agent_tests.rs"]
mod agent_tests;

#[cfg(test)]
#[path = "store/delivery_tests.rs"]
mod delivery_tests;

#[cfg(test)]
#[path = "store/schema_tests.rs"]
mod schema_tests;

#[cfg(test)]
#[path = "store/adoption_tests.rs"]
mod adoption_tests;

pub fn open(path: &Path) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let mut connection = rusqlite::Connection::open(path)?;

    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    // Before WAL, which writes the db header: another program's db is refused unchanged.
    migrate(&mut connection)?;
    connection.execute_batch("PRAGMA journal_mode=WAL;")?;
    connection.pragma_update(None, "foreign_keys", true)?;

    Ok(connection)
}

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001.sql"),
    include_str!("../migrations/0002.sql"),
    include_str!("../migrations/0003.sql"),
    include_str!("../migrations/0004.sql"),
    include_str!("../migrations/0005.sql"),
    include_str!("../migrations/0006.sql"),
    include_str!("../migrations/0007.sql"),
    include_str!("../migrations/0008.sql"),
    include_str!("../migrations/0009.sql"),
    include_str!("../migrations/0010.sql"),
    include_str!("../migrations/0011.sql"),
    include_str!("../migrations/0012.sql"),
    include_str!("../migrations/0013.sql"),
    include_str!("../migrations/0014.sql"),
];

#[derive(serde::Serialize)]
pub struct Notice {
    pub id: i64,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub title: String,
    pub body: String,
}

pub fn add_notice(
    connection: &Connection,
    session: Option<&str>,
    agent: Option<&str>,
    title: &str,
    body: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO notice (session_id, agent_id, title, body) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session, agent, title, body],
    )?;
    Ok(())
}

/// The write lock covers both the read and the mark, so concurrent app reads cannot repeat a row.
pub fn take_notices(connection: &mut Connection) -> rusqlite::Result<Vec<Notice>> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let notices = {
        let mut query = tx.prepare("SELECT id, session_id, agent_id, title, body FROM notice WHERE posted_at IS NULL ORDER BY id")?;
        query
            .query_map([], |row| {
                Ok(Notice {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    title: row.get(3)?,
                    body: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    tx.execute(
        "UPDATE notice SET posted_at = unixepoch() WHERE posted_at IS NULL",
        [],
    )?;
    tx.commit()?;
    Ok(notices)
}

fn known_version(version: i64) -> bool {
    (1..=MIGRATIONS.len() as i64).contains(&version)
}

/// Whether `path` is a db that a swarm made: it shows a known `user_version` and every table of
/// migration 0001. A home from before the marker is adopted on this (ADR 0036). The open is
/// `immutable`, because a plain read-only open of a WAL db makes `-shm` and `-wal` files in a
/// folder that may not be swarm's. It skips changes still in a `-wal`; swarm's schema was
/// checkpointed long ago, and a miss is a refusal whose message gives the fix.
pub fn made_by_swarm(path: &Path) -> bool {
    // An absolute path after `file://`: a leading `//` would else read as a URI authority.
    let Ok(path) = std::path::absolute(path) else {
        return false;
    };
    let escaped = path
        .to_string_lossy()
        .replace('%', "%25")
        .replace('?', "%3f")
        .replace('#', "%23");
    let Ok(connection) = Connection::open_with_flags(
        format!("file://{escaped}?mode=ro&immutable=1"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return false;
    };
    let version = connection.query_row("PRAGMA user_version", [], |row| row.get(0));
    let tables = connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table'
         AND name IN ('session', 'agent', 'message', 'read_mark', 'job')",
        [],
        |row| row.get::<_, i64>(0),
    );
    matches!((version, tables), (Ok(version), Ok(5)) if known_version(version))
}

fn migrate(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    // A db at the newest version needs no write, so its open waits behind no writer.
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == MIGRATIONS.len() as i64 {
        return Ok(());
    }
    // Migration 0013 rebuilds `agent`. SQLite's rebuild steps need foreign keys off, and a
    // transaction cannot change that, so it is off for the whole migration and checked before
    // the commit.
    let keys: bool = connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    connection.pragma_update(None, "foreign_keys", false)?;
    let migrated = migrate_locked(connection);
    connection.pragma_update(None, "foreign_keys", keys)?;
    migrated
}

fn migrate_locked(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    // Immediate: a read that later upgrades to a write fails at once when another swarm migrated
    // first; taking the write lock before the read waits on busy_timeout instead. The version is
    // read again under the lock, because another swarm may have migrated since.
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let version: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 {
        // A new db has no schema. Tables at version 0 belong to another program (ADR 0036).
        let objects: i64 =
            tx.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))?;
        if objects > 0 {
            return Err("database not made by swarm; use another SWARM_HOME or move it".into());
        }
        tx.execute_batch(MIGRATIONS[0])?;
    } else if !known_version(version) {
        return Err(
            "database made by another swarm build; use another SWARM_HOME or delete it".into(),
        );
    }
    // Counted only in a db that is swarm's: another program's keys may not even parse. Only a key
    // that the migration breaks refuses it; an older broken row is not its fault.
    let broken = |tx: &rusqlite::Transaction| -> rusqlite::Result<i64> {
        tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
    };
    let broken_before = broken(&tx)?;
    if version < 2 {
        tx.execute_batch(MIGRATIONS[1])?;
    }
    if version < 3 {
        tx.execute_batch(MIGRATIONS[2])?;
    }
    if version < 4 {
        tx.execute_batch(MIGRATIONS[3])?;
        tx.pragma_update(None, "user_version", 4)?;
    }
    if version < 5 {
        tx.execute_batch(MIGRATIONS[4])?;
        tx.pragma_update(None, "user_version", 5)?;
    }
    if version < 6 {
        tx.execute_batch(MIGRATIONS[5])?;
        tx.pragma_update(None, "user_version", 6)?;
    }

    if version < 7 {
        tx.execute_batch(MIGRATIONS[6])?;
        tx.pragma_update(None, "user_version", 7)?;
    }

    if version < 8 {
        tx.execute_batch(MIGRATIONS[7])?;
        tx.pragma_update(None, "user_version", 8)?;
    }

    if version < 9 {
        tx.execute_batch(MIGRATIONS[8])?;
        tx.pragma_update(None, "user_version", 9)?;
    }

    if version < 10 {
        tx.execute_batch(MIGRATIONS[9])?;
        tx.pragma_update(None, "user_version", 10)?;
    }

    if version < 11 {
        tx.execute_batch(MIGRATIONS[10])?;
        tx.pragma_update(None, "user_version", 11)?;
    }

    if version < 12 {
        tx.execute_batch(MIGRATIONS[11])?;
        tx.pragma_update(None, "user_version", 12)?;
    }

    if version < 13 {
        tx.execute_batch(MIGRATIONS[12])?;
        tx.pragma_update(None, "user_version", 13)?;
    }

    if version < 14 {
        tx.execute_batch(MIGRATIONS[13])?;
        tx.pragma_update(None, "user_version", 14)?;
    }

    let added = broken(&tx)? - broken_before;
    if added > 0 {
        return Err(format!(
            "migration would break a foreign key in {added} rows, so it changed nothing; report this swarm bug"
        )
        .into());
    }

    tx.commit()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) const SESSION: &str = "0199a000-0000-7000-8000-000000000001";
    pub(super) const OTHER_SESSION: &str = "0199a000-0000-7000-8000-000000000002";
    pub(super) const ORCHESTRATOR: &str = "orchestrator";
    pub(super) const CODER: &str = "coder";
    pub(super) const OUTSIDER: &str = "outsider";

    pub(super) fn seed() -> Connection {
        let connection = open(Path::new(":memory:")).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test'), ('{OTHER_SESSION}', 'lane', '/other');
                 INSERT INTO agent (id, session_id, role) VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'), ('{CODER}', '{SESSION}', 'coder'), ('{OUTSIDER}', '{OTHER_SESSION}', 'coder');"
            ))
            .unwrap();
        connection
    }

    pub(super) fn unix_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    pub(super) fn temp_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("swarm-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    pub(super) fn usage_fixture(tag: &str) -> (std::path::PathBuf, Connection) {
        let root = temp_root(tag);
        std::fs::create_dir_all(&root).unwrap();
        let connection = open(&root.join("swarm.db")).unwrap();
        connection
            .execute(
                "INSERT INTO session (id, talk_mode, cwd) VALUES (?1, 'lane', ?2)",
                (SESSION, root.to_string_lossy()),
            )
            .unwrap();
        add_agent(&connection, SESSION, CODER, "coder").unwrap();
        set_log(&connection, SESSION, CODER, &root.join("chat.jsonl")).unwrap();
        (root, connection)
    }

    pub(super) fn assistant_usage(id: &str, model: &str) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":4}}}}}}"#
        )
    }

    #[test]
    fn claude_usage_grows_once_per_message_across_reads() {
        use std::io::Write;
        let (root, connection) = usage_fixture("usage-growing");
        let path = root.join("chat.jsonl");
        let first = assistant_usage("m1", "claude-first");
        std::fs::write(
            &path,
            format!(
                "{}\n{first}\n{first}\n",
                include_str!("../packages/transcript/src/fixtures/claude-chair-run.jsonl")
            ),
        )
        .unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(64));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(
            file,
            "{}\n{first}\n{}",
            assistant_usage("m2", "claude-new"),
            assistant_usage("stub", "<synthetic>")
        )
        .unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let agent = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(agent.tokens, Some(128));
        assert_eq!(agent.model.as_deref(), Some("claude-first"));
        assert_eq!(agent.cost_usd, None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
