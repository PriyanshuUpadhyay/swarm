//! Database adoption and older-schema tests.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    /// A ring from an older build was never checked, and its pane has moved on since, so the
    /// migration stores it as unchecked. A later pass neither settles it nor reports it lost to
    /// the chair (ADR 0041).
    #[test]
    fn a_version_four_ring_is_unchecked_and_never_reported_lost() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..4] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 4).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role)
                     VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'),
                            ('{CODER}', '{SESSION}', 'coder');
                 INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path,
                                      rung_at, rings)
                     VALUES ('{SESSION}', 0, '{ORCHESTRATOR}', '{CODER}', 'ask', 'runs/0.txt',
                             1700, 2),
                            ('{SESSION}', 1, '{ORCHESTRATOR}', '{CODER}', 'ask', 'runs/1.txt',
                             NULL, 0);"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        assert!(unsettled_rings(&connection, SESSION).unwrap().is_empty());
        assert!(
            unreported_lost(&connection, SESSION, ORCHESTRATOR)
                .unwrap()
                .is_empty()
        );
        let delivery: Vec<Option<String>> = messages(&connection, SESSION, -1)
            .unwrap()
            .into_iter()
            .map(|message| message.delivery)
            .collect();
        assert_eq!(delivery, [Some("unchecked".to_string()), None]);
    }

    /// `swarm send` takes any kind, so an older database can hold two hand-sent messages of one
    /// report kind. The migration keeps both, and only a new report of that kind is refused.
    #[test]
    fn a_version_four_database_with_two_hand_sent_report_kinds_migrates_and_keeps_both() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..4] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 4).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role)
                     VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'),
                            ('{CODER}', '{SESSION}', 'coder');
                 INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path)
                     VALUES ('{SESSION}', 0, '{CODER}', '{ORCHESTRATOR}', 'stall:x', 'runs/0.txt'),
                            ('{SESSION}', 1, '{CODER}', '{ORCHESTRATOR}', 'stall:x', 'runs/1.txt');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        let kept: Vec<(i64, String)> = messages(&connection, SESSION, -1)
            .unwrap()
            .into_iter()
            .map(|message| (message.seq, message.kind))
            .collect();
        assert_eq!(
            kept,
            [(0, "stall:x".to_string()), (1, "stall:x".to_string())]
        );
        let root = std::env::temp_dir().join(format!("swarm-store-{}", uuid::Uuid::now_v7()));
        let resend = send_message(
            &mut connection,
            &root,
            SESSION,
            CODER,
            ORCHESTRATOR,
            "stall:x",
            "again",
        );
        let _ = std::fs::remove_dir_all(&root);
        // `report` in main.rs reads this code as a report already sent.
        assert!(matches!(
            resend.unwrap_err().downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(failure, _))
                if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER
        ));
    }

    #[test]
    fn a_version_three_database_gains_a_chat_log_that_outlives_the_pane() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..3] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 3).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role) VALUES ('{CODER}', '{SESSION}', 'coder');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].log, None);
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_log(&connection, SESSION, CODER, Path::new("/logs/coder.jsonl")).unwrap();
        clear_pane(&connection, SESSION, CODER).unwrap();
        assert_eq!(
            agents(&connection, SESSION).unwrap()[0].log.as_deref(),
            Some("/logs/coder.jsonl")
        );
    }

    #[test]
    fn rejects_database_from_another_build() {
        let root = temp_root("foreign-version");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 14")
            .unwrap();
        drop(connection);

        assert_eq!(
            open(&db).unwrap_err().to_string(),
            "database made by another swarm build; use another SWARM_HOME or delete it"
        );
        Connection::open(&db)
            .unwrap()
            .pragma_update(None, "user_version", MIGRATIONS.len() + 1)
            .unwrap();
        assert!(open(&db).is_err());
    }

    /// The probe that decides adoption writes nothing, also for a WAL db, which a read-only
    /// open would give `-shm` and `-wal` files (ADR 0036).
    #[test]
    fn the_adoption_probe_leaves_another_programs_wal_database_as_it_was() {
        let root = temp_root("other-program-wal");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE bookmark (url TEXT);")
            .unwrap();
        let listing = || {
            let mut names: Vec<_> = std::fs::read_dir(&root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let (names, bytes) = (listing(), std::fs::read(&db).unwrap());

        assert!(!made_by_swarm(&db));
        assert_eq!(listing(), names);
        // A path with a leading `//` or a relative one names the same db.
        let mine = root.join("mine.db");
        open(&mine).unwrap();
        let double = std::path::PathBuf::from(format!("/{}", mine.display()));
        let relative = pathdiff_from_cwd(&mine);
        for path in [&mine, &double, &relative] {
            assert!(made_by_swarm(path), "{}", path.display());
        }
        assert!(open(&db).is_err());
        assert_eq!(std::fs::read(&db).unwrap(), bytes);
    }

    /// `path` written relative to the current directory, through `..` up to `/`.
    fn pathdiff_from_cwd(path: &Path) -> std::path::PathBuf {
        let cwd = std::env::current_dir().unwrap();
        let up = cwd.components().count() - 1;
        let mut relative: std::path::PathBuf = std::iter::repeat_n("..", up).collect();
        relative.push(path.strip_prefix("/").unwrap());
        relative
    }

    #[test]
    fn another_programs_database_gets_no_swarm_tables() {
        let root = temp_root("other-program");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch("CREATE TABLE bookmark (url TEXT)")
            .unwrap();

        assert!(!made_by_swarm(&db));
        assert_eq!(
            open(&db).unwrap_err().to_string(),
            "database not made by swarm; use another SWARM_HOME or move it"
        );
        let tables: Vec<String> = Connection::open(&db)
            .unwrap()
            .prepare("SELECT name FROM sqlite_master")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(tables, ["bookmark"]);
        open(&root.join("new.db")).unwrap();
        assert!(made_by_swarm(&root.join("new.db")));
    }

    /// A key that SQLite cannot check, in another program's db, still gets the ADR 0036 refusal,
    /// because migrate counts broken keys only in a db it knows is swarm's.
    #[test]
    fn another_programs_database_with_a_bad_key_gets_the_refusal() {
        let root = temp_root("other-program-key");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch("CREATE TABLE p (a, b); CREATE TABLE ch (x REFERENCES p(a));")
            .unwrap();
        assert_eq!(
            open(&db).unwrap_err().to_string(),
            "database not made by swarm; use another SWARM_HOME or move it"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A second swarm that opens the db while the first migrates it waits and then finds it
    /// migrated, not "database is locked", because migrate takes the write lock before it reads
    /// the version.
    #[test]
    fn a_second_open_waits_for_a_migration_in_progress() {
        let root = temp_root("migrate-race");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        let first = Connection::open(&db).unwrap();
        for migration in &MIGRATIONS[..5] {
            first.execute_batch(migration).unwrap();
        }
        first.pragma_update(None, "user_version", 5).unwrap();
        first.execute_batch("BEGIN IMMEDIATE;").unwrap();
        for migration in &MIGRATIONS[5..] {
            first.execute_batch(migration).unwrap();
        }
        first
            .pragma_update(None, "user_version", MIGRATIONS.len())
            .unwrap();
        let committer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            first.execute_batch("COMMIT;").unwrap();
        });
        let second = open(&db);
        committer.join().unwrap();
        assert!(second.is_ok(), "{:?}", second.err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// An open of a db at the newest version takes no write lock, so a polled read does not wait
    /// behind a writer.
    #[test]
    fn an_open_of_a_current_db_does_not_wait_for_a_writer() {
        let root = temp_root("migrate-current");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        drop(open(&db).unwrap());
        let writer = Connection::open(&db).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let reader = open(&db);
        assert!(reader.is_ok(), "{:?}", reader.err());
        writer.execute_batch("ROLLBACK;").unwrap();
        std::fs::remove_dir_all(&root).unwrap();
    }
}
