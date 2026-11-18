use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

mod agent_state;
mod agents;
mod board;
mod board_claims;
mod board_ping;
mod board_read;
mod delivery;
mod messages;
mod sessions;
mod usage;
pub use agent_state::*;
pub use agents::*;
pub use board::*;
pub use board_claims::*;
pub use board_ping::*;
pub use board_read::*;
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
#[path = "store/board_test_support.rs"]
mod board_test_support;

#[cfg(test)]
#[path = "store/board_tests.rs"]
mod board_tests;

#[cfg(test)]
#[path = "store/board_claim_tests.rs"]
mod board_claim_tests;

#[cfg(test)]
#[path = "store/board_ping_tests.rs"]
mod board_ping_tests;

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
    include_str!("../migrations/0015.sql"),
    include_str!("../migrations/0016.sql"),
    include_str!("../migrations/0017.sql"),
];

/// ADR 0147: the kinds of a notice row. The HUD decodes the same words (`Inbox.swift`).
pub mod notice_kind {
    pub const DONE: &str = "done";
    /// A chair reply that ends with a question, or a board decision that waits for the owner.
    pub const ASKS: &str = "asks";
    pub const FAILED: &str = "failed";
    pub const WAITING: &str = "waiting";
    /// The Settings test notice, which checks macOS delivery.
    pub const TEST: &str = "test";
}

#[derive(serde::Serialize)]
pub struct Notice {
    pub id: i64,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub title: String,
    pub body: String,
    /// One of `notice_kind`, and more may come; None for an old row.
    pub kind: Option<String>,
}

impl Notice {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Notice {
            id: row.get(0)?,
            session_id: row.get(1)?,
            agent_id: row.get(2)?,
            title: row.get(3)?,
            body: row.get(4)?,
            kind: row.get(5)?,
        })
    }
}

/// The columns of `Notice::from_row`, in its order.
const NOTICE_COLUMNS: &str = "id, session_id, agent_id, title, body, kind";

pub fn add_notice(
    connection: &Connection,
    session: Option<&str>,
    agent: Option<&str>,
    kind: Option<&str>,
    title: &str,
    body: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO notice (session_id, agent_id, kind, title, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![session, agent, kind, title, body],
    )?;
    Ok(())
}

/// The write lock covers both the read and the mark, so concurrent app reads cannot repeat a row.
pub fn take_notices(connection: &mut Connection) -> rusqlite::Result<Vec<Notice>> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let notices = {
        let mut query = tx.prepare(&format!(
            "SELECT {NOTICE_COLUMNS} FROM notice WHERE posted_at IS NULL ORDER BY id"
        ))?;
        query
            .query_map([], Notice::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    tx.execute(
        "UPDATE notice SET posted_at = unixepoch() WHERE posted_at IS NULL",
        [],
    )?;
    tx.commit()?;
    Ok(notices)
}

/// ADR 0147: the newest unseen done, asks, or failed row of each chat, newest first, at most
/// `limit`, and the number of such chats. A session that a newer one continues is part of that
/// chat, so its rows do not count. A wait is not here: the HUD reads waiting agents live, so an
/// answered wait never stays.
pub fn notice_inbox(
    connection: &Connection,
    limit: usize,
) -> rusqlite::Result<(Vec<Notice>, usize)> {
    use notice_kind::{ASKS, DONE, FAILED};
    let mut query = connection.prepare(&format!(
        "SELECT {NOTICE_COLUMNS}, count(*) OVER () AS total FROM notice
         WHERE id IN (SELECT max(id) FROM notice
                      WHERE seen_at IS NULL AND session_id IS NOT NULL AND kind IN (?2, ?3, ?4)
                        AND session_id NOT IN (SELECT continuation_of FROM session
                                               WHERE continuation_of IS NOT NULL)
                      GROUP BY session_id)
         ORDER BY id DESC LIMIT ?1"
    ))?;

    let mut total = 0;
    let rows = query
        .query_map(rusqlite::params![limit as i64, DONE, ASKS, FAILED], |row| {
            total = row.get::<_, i64>("total")? as usize;
            Notice::from_row(row)
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok((rows, total))
}

/// Marks the rows of these sessions seen, only up to row `through` when it is given, so a row that
/// came after the HUD's read stays unseen (ADR 0147). Returns how many rows changed.
pub fn mark_notices_seen(
    connection: &Connection,
    sessions: &[&str],
    through: Option<i64>,
) -> rusqlite::Result<usize> {
    let marks = vec!["?"; sessions.len()].join(", ");
    let mut values: Vec<rusqlite::types::Value> = vec![through.unwrap_or(i64::MAX).into()];
    values.extend(sessions.iter().map(|session| session.to_string().into()));
    connection.execute(
        &format!(
            "UPDATE notice SET seen_at = unixepoch()
             WHERE seen_at IS NULL AND id <= ?1 AND session_id IN ({marks})"
        ),
        rusqlite::params_from_iter(values),
    )
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
    // Migrations 0013 and 0016 rebuild `agent`. SQLite's rebuild steps need foreign keys off, and a
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

    if version < 15 {
        tx.execute_batch(MIGRATIONS[14])?;
        tx.pragma_update(None, "user_version", 15)?;
    }

    if version < 16 {
        tx.execute_batch(MIGRATIONS[15])?;
        tx.pragma_update(None, "user_version", 16)?;
    }

    if version < 17 {
        tx.execute_batch(MIGRATIONS[16])?;
        tx.pragma_update(None, "user_version", 17)?;
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

    /// ADR 0147: the HUD shows "+N more" from the full count, not from the cut list.
    #[test]
    fn the_inbox_counts_every_unseen_chat_past_its_limit() {
        let connection = seed();
        for session in [SESSION, OTHER_SESSION] {
            let done = Some(notice_kind::DONE);
            add_notice(&connection, Some(session), None, done, "Chat", "Done.").unwrap();
        }

        let (rows, total) = notice_inbox(&connection, 1).unwrap();

        assert_eq!((rows.len(), total), (1, 2));

        // A continued chat is one chat, so only its newest session's row counts.
        connection
            .execute(
                "UPDATE session SET continuation_of = ?1 WHERE id = ?2",
                (SESSION, OTHER_SESSION),
            )
            .unwrap();
        let (rows, total) = notice_inbox(&connection, 20).unwrap();
        assert_eq!((rows.len(), total), (1, 1));
        assert_eq!(rows[0].session_id.as_deref(), Some(OTHER_SESSION));
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
                include_str!("../tests/fixtures/provider-logs/claude-chair-run.jsonl")
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
