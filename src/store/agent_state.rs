//! Agent state reports and guarded screen updates.

use super::*;

/// Record an agent's reported state; `now` is unix seconds. The table's CHECKs refuse unknown
/// states, sources, and invalid wait kinds. A kindless wait keeps the kind of an existing wait;
/// every other state write replaces it. Returns `Some` with the state it replaced (None when the
/// agent had no state yet), read in the same immediate transaction, so two hooks that race cannot both see one
/// change (a notice is sent once per change, ADR 0044). Returns None and writes nothing when the
/// session holds no such agent.
#[allow(clippy::too_many_arguments)]
pub fn set_state(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    state: &str,
    source: &str,
    detail: Option<&str>,
    kind: Option<&str>,
    now: i64,
) -> Result<Option<Option<String>>, Box<dyn std::error::Error>> {
    use rusqlite::OptionalExtension;
    let transaction =
        rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    let Some(old) = transaction
        .query_row(
            "SELECT state FROM agent WHERE session_id = ?1 AND id = ?2",
            (session_id, agent_id),
            |row| row.get(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    transaction.execute(
        "UPDATE agent SET state = ?3, state_source = ?4, state_detail = ?5,
            state_kind = CASE WHEN ?3 = 'waiting' AND ?6 IS NULL AND state = 'waiting'
                              THEN state_kind ELSE ?6 END, state_at = ?7
         WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id, state, source, detail, kind, now),
    )?;
    transaction.commit()?;
    Ok(Some(old))
}

/// The report a listing read for one agent: (state, state_source, state_at, state_kind).
pub type ReadState<'a> = (
    Option<&'a str>,
    Option<&'a str>,
    Option<i64>,
    Option<&'a str>,
);

/// A screen check's write, applied only while the row still holds the report it read: a hook
/// that lands between the listing's read and this write keeps its newer report, also within the
/// same second, because state, source, and kind take part in the compare. Returns whether the row
/// changed.
pub fn set_screen_state(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    state: &str,
    detail: Option<&str>,
    now: i64,
    read: ReadState<'_>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (read_state, read_source, read_at, read_kind) = read;
    let changed = connection.execute(
        "UPDATE agent SET state = ?3, state_source = 'screen', state_detail = ?4, state_kind = NULL, state_at = ?5
         WHERE session_id = ?1 AND id = ?2
           AND state IS ?6 AND state_source IS ?7 AND state_at IS ?8 AND state_kind IS ?9",
        (
            session_id,
            agent_id,
            state,
            detail,
            now,
            read_state,
            read_source,
            read_at,
            read_kind,
        ),
    )?;
    Ok(changed == 1)
}

/// Records that the owner sent `agent_id` a prompt at `at`, or None when the send failed; see
/// `screen::held_by_send`.
pub fn mark_sent(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    at: Option<i64>,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.execute(
        "UPDATE agent SET sent_at = ?3 WHERE session_id = ?1 AND id = ?2",
        (session_id, agent_id, at),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn sets_state_only_for_one_agent_and_refuses_unknown_values() {
        let connection = seed(0);
        set_state(
            &connection,
            SESSION,
            CODER,
            "failed",
            "hook",
            Some("rate_limit"),
            None,
            42,
        )
        .unwrap();
        let rows = agents(&connection, SESSION).unwrap();
        let coder = rows.iter().find(|row| row.id == CODER).unwrap();
        let orchestrator = rows.iter().find(|row| row.id == ORCHESTRATOR).unwrap();
        assert_eq!(coder.state.as_deref(), Some("failed"));
        assert_eq!(coder.state_detail.as_deref(), Some("rate_limit"));
        assert_eq!(orchestrator.state, None);
        assert_eq!(
            set_state(
                &connection,
                SESSION,
                ORCHESTRATOR,
                "done",
                "hook",
                None,
                None,
                42
            )
            .unwrap(),
            Some(None)
        );
        assert_eq!(
            set_state(
                &connection,
                SESSION,
                "ghost",
                "waiting",
                "hook",
                None,
                None,
                42
            )
            .unwrap(),
            None
        );
        assert!(
            set_state(
                &connection,
                SESSION,
                CODER,
                "asleep",
                "hook",
                None,
                None,
                42
            )
            .is_err()
        );
        assert!(set_state(&connection, SESSION, CODER, "done", "guess", None, None, 42).is_err());
    }

    #[test]
    fn a_waiting_hook_records_kind_and_a_screen_write_clears_it() {
        let connection = seed(0);
        for (kind, screen_state) in [("permission", "waiting"), ("question", "done")] {
            set_state(
                &connection,
                SESSION,
                CODER,
                "waiting",
                "hook",
                Some("prompt"),
                Some(kind),
                100,
            )
            .unwrap();
            let coder = &agents(&connection, SESSION).unwrap()[0];
            assert_eq!(coder.state_kind.as_deref(), Some(kind));
            assert_eq!(coder.state_detail.as_deref(), Some("prompt"));
            assert!(
                set_screen_state(
                    &connection,
                    SESSION,
                    CODER,
                    screen_state,
                    None,
                    110,
                    (Some("waiting"), Some("hook"), Some(100), Some(kind)),
                )
                .unwrap()
            );
            let coder = &agents(&connection, SESSION).unwrap()[0];
            assert_eq!(coder.state_kind, None);
            assert_eq!(coder.state.as_deref(), Some(screen_state));
        }
    }

    #[test]
    fn state_kind_checks_reject_non_waiting_states_and_unknown_kinds() {
        let connection = seed(0);
        for (state, kind) in [("done", "permission"), ("waiting", "unknown")] {
            let error = set_state(
                &connection,
                SESSION,
                CODER,
                state,
                "hook",
                None,
                Some(kind),
                100,
            )
            .unwrap_err();
            assert!(error.to_string().contains("CHECK constraint failed"));
            assert_eq!(agents(&connection, SESSION).unwrap()[0].state, None);
        }
    }

    #[test]
    fn a_kindless_wait_keeps_its_kind_until_the_state_changes() {
        let connection = seed(0);
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            Some("permission"),
            100,
        )
        .unwrap();
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            None,
            101,
        )
        .unwrap();
        assert_eq!(
            agents(&connection, SESSION).unwrap()[0]
                .state_kind
                .as_deref(),
            Some("permission")
        );
        set_state(
            &connection,
            SESSION,
            CODER,
            "working",
            "hook",
            None,
            None,
            102,
        )
        .unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].state_kind, None);
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            None,
            103,
        )
        .unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].state_kind, None);
    }

    #[test]
    fn a_same_second_kind_change_stops_the_screen_write() {
        let connection = seed(0);
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            Some("permission"),
            100,
        )
        .unwrap();
        let read = (Some("waiting"), Some("hook"), Some(100), Some("permission"));
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            Some("question"),
            100,
        )
        .unwrap();
        assert!(!set_screen_state(&connection, SESSION, CODER, "done", None, 100, read).unwrap());
        let coder = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(coder.state.as_deref(), Some("waiting"));
        assert_eq!(coder.state_kind.as_deref(), Some("question"));
    }

    #[test]
    fn a_hook_state_without_kind_clears_the_previous_wait_kind() {
        let connection = seed(0);
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            Some("permission"),
            100,
        )
        .unwrap();
        set_state(&connection, SESSION, CODER, "done", "hook", None, None, 110).unwrap();
        let coder = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(coder.state.as_deref(), Some("done"));
        assert_eq!(coder.state_kind, None);
    }

    #[test]
    fn a_screen_write_does_not_overwrite_a_newer_hook_report() {
        let connection = seed(0);
        set_state(
            &connection,
            SESSION,
            CODER,
            "working",
            "hook",
            None,
            None,
            100,
        )
        .unwrap();
        // The listing read state_at 100; a hook reports waiting before the screen write lands.
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            None,
            105,
        )
        .unwrap();
        let read = (Some("working"), Some("hook"), Some(100), None);
        let written =
            set_screen_state(&connection, SESSION, CODER, "done", None, 110, read).unwrap();
        assert!(!written);
        let coder = agents(&connection, SESSION)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.state.as_deref(), Some("waiting"));
        assert_eq!(coder.state_source.as_deref(), Some("hook"));
        let latest = (Some("waiting"), Some("hook"), Some(105), None);
        assert!(set_screen_state(&connection, SESSION, CODER, "done", None, 111, latest).unwrap());
        let unset = (None, None, None, None);
        assert!(
            set_screen_state(&connection, SESSION, ORCHESTRATOR, "done", None, 112, unset).unwrap()
        );
    }

    #[test]
    fn a_hook_in_the_same_second_as_the_read_still_stops_the_screen_write() {
        let connection = seed(0);
        set_screen_state(
            &connection,
            SESSION,
            CODER,
            "working",
            None,
            100,
            (None, None, None, None),
        )
        .unwrap();
        // The listing read (working, screen, 100); a hook reports waiting in that same second.
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            None,
            None,
            100,
        )
        .unwrap();
        let read = (Some("working"), Some("screen"), Some(100), None);
        assert!(!set_screen_state(&connection, SESSION, CODER, "done", None, 100, read).unwrap());
        let coder = agents(&connection, SESSION)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.state.as_deref(), Some("waiting"));
    }

    #[test]
    fn closing_a_pane_clears_the_agents_state() {
        let connection = seed(0);
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_state(
            &connection,
            SESSION,
            CODER,
            "waiting",
            "hook",
            Some("quota"),
            Some("permission"),
            42,
        )
        .unwrap();
        clear_pane(&connection, SESSION, CODER).unwrap();
        let coder = agents(&connection, SESSION)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.pane, None);
        assert_eq!(
            (
                coder.state,
                coder.state_at,
                coder.state_source,
                coder.state_detail,
                coder.state_kind
            ),
            (None, None, None, None, None)
        );
    }
}
