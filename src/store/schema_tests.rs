//! Schema migration tests.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn migration_adds_notices_to_a_version_eight_database() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..8] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 8).unwrap();
        migrate(&mut connection).unwrap();
        add_notice(&connection, Some("session"), Some("chair"), "Done", "body").unwrap();
        let notices = take_notices(&mut connection).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].session_id.as_deref(), Some("session"));
        assert_eq!(notices[0].agent_id.as_deref(), Some("chair"));
        assert_eq!(notices[0].title, "Done");
        assert!(take_notices(&mut connection).unwrap().is_empty());
    }

    #[test]
    fn migration_adds_chat_continuations_to_existing_databases() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        migrate(&mut connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        let old = create_session(&connection, "lane", Path::new("/work"), None, None).unwrap();
        let new = create_session(&connection, "lane", Path::new("/work"), None, None).unwrap();
        continue_session(&connection, &new, &old).unwrap();
        assert_eq!(
            sessions(&connection).unwrap()[0].continuation_of.as_deref(),
            Some(old.as_str())
        );
        assert!(continue_session(&connection, &old, &new).is_err());
        let branch = create_session(&connection, "lane", Path::new("/work"), None, None).unwrap();
        branch_session(&connection, &branch, &old, Some(("pi", "b-orchestrator"))).unwrap();
        let row = sessions(&connection).unwrap().remove(0);
        assert_eq!(row.branch_of.as_deref(), Some(old.as_str()));
        assert_eq!(row.chair_id.as_deref(), Some("b-orchestrator"));
        assert_eq!(row.continuation_of, None);
        assert!(branch_session(&connection, &branch, &old, None).is_err());
    }

    #[test]
    fn migration_adds_agent_state_to_a_version_two_database() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection.execute_batch(MIGRATIONS[1]).unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role) VALUES ('{CODER}', '{SESSION}', 'coder');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        let coder = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(coder.state, None);
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            None,
            1_700,
        )
        .unwrap();
        let coder = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(coder.state.as_deref(), Some("waiting"));
        assert_eq!(coder.state_at, Some(1_700));
        assert_eq!(coder.state_source.as_deref(), Some("hook"));
    }

    #[test]
    fn migration_adds_state_kind_to_a_version_nine_database() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..9] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 9).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role, state)
                     VALUES ('{CODER}', '{SESSION}', 'coder', 'waiting');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        let kind: Option<String> = connection
            .query_row("SELECT state_kind FROM agent", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kind, None);
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    /// Migration 0013 rebuilds `agent` so a host record (ADR 0070) can be its state source. Every
    /// row, key, and other check survives the rebuild.
    #[test]
    fn migration_lets_a_version_twelve_database_store_a_host_state() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..12] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 12).unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO agent (id, session_id, role, pane_id, state, state_source, state_at, runner, sent_at)
                     VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator', '%1', 'done', 'screen', 5, NULL, NULL),
                            ('{CODER}', '{SESSION}', 'coder', '%2', 'working', 'hook', 7, 'claude/opus/high', 9);
                 INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path)
                     VALUES ('{SESSION}', 0, '{ORCHESTRATOR}', '{CODER}', 'ask', 'runs/0.txt');
                 INSERT INTO read_mark (session_id, message_seq, agent_id) VALUES ('{SESSION}', 0, '{CODER}');
                 INSERT INTO job (session_id, agent_id, kind) VALUES ('{SESSION}', '{CODER}', 'build');"
            ))
            .unwrap();
        migrate(&mut connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        type Row = (
            String,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<i64>,
        );
        let rows: Vec<Row> = connection
            .prepare("SELECT id, pane_id, state_source, state_at, runner, sent_at FROM agent ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            [
                (
                    CODER.to_string(),
                    Some("%2".into()),
                    Some("hook".into()),
                    Some(7),
                    Some("claude/opus/high".into()),
                    Some(9)
                ),
                (
                    ORCHESTRATOR.to_string(),
                    Some("%1".into()),
                    Some("screen".into()),
                    Some(5),
                    None,
                    None
                ),
            ]
        );
        set_state(&connection, SESSION, CODER, "done", "host", None, None, 11).unwrap();
        assert!(set_state(&connection, SESSION, CODER, "done", "guess", None, None, 12).is_err());
        assert!(
            connection
                .execute(
                    "UPDATE agent SET state = 'done', state_kind = 'permission' WHERE id = ?1",
                    [CODER]
                )
                .is_err()
        );
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        let broken: i64 = connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(broken, 0);
        assert!(
            connection
                .execute(
                    &format!("INSERT INTO job (session_id, agent_id, kind) VALUES ('{SESSION}', 'ghost', 'build')"),
                    []
                )
                .is_err()
        );
    }

    /// A broken key from before the migration, such as a row an older build or a hand edit left,
    /// is not the rebuild's fault, so it does not refuse the database.
    #[test]
    fn migration_keeps_a_database_whose_old_rows_already_break_a_key() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..12] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 12).unwrap();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .execute_batch(&format!(
                "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
                 INSERT INTO job (session_id, agent_id, kind) VALUES ('{SESSION}', 'gone', 'build');"
            ))
            .unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrate(&mut connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn migration_renames_the_tmux_solo_adapter_to_tmux_swarm() {
        let mut connection = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..10] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 10).unwrap();
        connection
            .execute_batch(
                "INSERT INTO session (id, talk_mode, cwd, adapter) VALUES ('solo', 'lane', '/t', 'tmux-solo');
                 INSERT INTO session (id, talk_mode, cwd, adapter) VALUES ('herdr', 'lane', '/t', 'herdr');",
            )
            .unwrap();
        migrate(&mut connection).unwrap();
        let adapters: Vec<String> = connection
            .prepare("SELECT adapter FROM session ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(adapters, ["herdr", "tmux-swarm"]);
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn usage_migration_keeps_existing_values_and_starts_without_a_cursor() {
        let root = temp_root("usage-migration");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("swarm.db");
        let connection = Connection::open(&path).unwrap();
        for migration in &MIGRATIONS[..7] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 7).unwrap();
        connection
            .execute(
                "INSERT INTO session (id, talk_mode, cwd) VALUES (?1, 'lane', '/test')",
                [SESSION],
            )
            .unwrap();
        connection.execute("INSERT INTO agent (id, session_id, role, tokens, cost_usd) VALUES (?1, ?2, 'coder', 42, 3)", (CODER, SESSION)).unwrap();
        drop(connection);
        let connection = open(&path).unwrap();
        let values = connection
            .query_row(
                "SELECT tokens, cost_usd, usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
                (SESSION, CODER),
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(values, (42, 3.0, None));
        assert_eq!(
            connection
                .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
                .optional()
                .unwrap(),
            None
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
