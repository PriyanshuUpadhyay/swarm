//! Agent state reports and guarded screen updates.

use super::*;

/// Record an agent's reported state; `now` is unix seconds. The table's CHECKs refuse unknown
/// states, sources, and invalid block kinds. A kindless block keeps the kind of an existing block;
/// every other state write replaces it. Returns `Some` with the state it replaced (None when the
/// agent had no state yet), read in the same immediate transaction, so two hooks that race cannot both see one
/// change (a notice is sent once per change, ADR 0044). Returns None and writes nothing when the
/// session holds no such agent.
///
/// An agent's own `done` does not replace its own `error`: a turn that an API error ends
/// (Claude `StopFailure`) is followed about 60 s later by Claude's `idle_prompt`, and in Rex by
/// an `idle` report, which would hide the failure. The next turn starts with `working`.
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

    let Some(old): Option<Option<String>> = transaction
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
        &format!(
            "UPDATE agent SET state = ?3, state_source = ?4, state_detail = ?5,
            state_kind = CASE WHEN ?3 = 'blocked' AND ?6 IS NULL AND state = 'blocked'
                              THEN state_kind ELSE ?6 END, state_at = ?7,
            turn_at = {}
         WHERE session_id = ?1 AND id = ?2
           AND NOT (?3 = 'done' AND ?4 IN ('hook', 'host')
                    AND state IS 'error' AND state_source IN ('hook', 'host'))",
            turn_at_sql("?7")
        ),
        (session_id, agent_id, state, source, detail, kind, now),
    )?;

    transaction.commit()?;
    // A turn reports `working` at each tool call; only a change of state moves the HUD.
    if old.as_deref() != Some(state) || state == "blocked" {
        crate::hud::signal_changed();
    }
    Ok(Some(old))
}

/// ADR 0147: the new `turn_at` of an update that sets state `?3` at time `now`. A move to `working`
/// from a state other than `working` or `blocked` starts a turn; an answered block goes on with it.
fn turn_at_sql(now: &str) -> String {
    format!(
        "CASE WHEN ?3 = 'working' AND state IS NOT 'working' AND state IS NOT 'blocked'
              THEN {now} ELSE turn_at END"
    )
}

/// When the agent's current turn began (ADR 0147), or None before its first turn; see
/// `turn_at_sql`. Both a hook report and a screen check can start a turn.
pub fn turn_at(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
) -> rusqlite::Result<Option<i64>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row(
            "SELECT turn_at FROM agent WHERE session_id = ?1 AND id = ?2",
            (session_id, agent_id),
            |row| row.get(0),
        )
        .optional()?
        .flatten())
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
        &format!("UPDATE agent SET state = ?3, state_source = 'screen', state_detail = ?4, state_kind = NULL, state_at = ?5,
            turn_at = {}
         WHERE session_id = ?1 AND id = ?2
           AND state IS ?6 AND state_source IS ?7 AND state_at IS ?8 AND state_kind IS ?9", turn_at_sql("?5")),
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

    if changed == 1 && read_state != Some(state) {
        crate::hud::signal_changed();
    }
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
        let connection = seed();
        set_state(
            &connection,
            SESSION,
            CODER,
            "error",
            "hook",
            Some("rate_limit"),
            None,
            42,
        )
        .unwrap();
        let rows = agents(&connection, SESSION).unwrap();
        let coder = rows.iter().find(|row| row.id == CODER).unwrap();
        let orchestrator = rows.iter().find(|row| row.id == ORCHESTRATOR).unwrap();
        assert_eq!(coder.state.as_deref(), Some("error"));
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
                "blocked",
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
        {
            let connection = seed();
            for (state, kind) in [("done", "permission"), ("blocked", "unknown")] {
                let refused = set_state(
                    &connection,
                    SESSION,
                    CODER,
                    state,
                    "hook",
                    None,
                    Some(kind),
                    100,
                )
                .is_err();
                assert!(refused);
                assert_eq!(agents(&connection, SESSION).unwrap()[0].state, None);
            }
        }
    }

    #[test]
    fn an_idle_report_after_an_api_error_keeps_the_failure() {
        let connection = seed();
        let set = |state, detail| {
            set_state(&connection, SESSION, CODER, state, "hook", detail, None, 42).unwrap();
            let rows = agents(&connection, SESSION).unwrap();
            let coder = rows.into_iter().find(|row| row.id == CODER).unwrap();
            (coder.state, coder.state_detail)
        };
        set("error", Some("server_error"));
        assert_eq!(
            set("done", None),
            (Some("error".into()), Some("server_error".into()))
        );
        assert_eq!(set("working", None).0.as_deref(), Some("working"));
    }

    #[test]
    fn a_waiting_hook_records_kind_and_a_screen_write_clears_it() {
        let connection = seed();
        for (kind, screen_state) in [("permission", "blocked"), ("question", "done")] {
            set_state(
                &connection,
                SESSION,
                CODER,
                "blocked",
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
                    (Some("blocked"), Some("hook"), Some(100), Some(kind)),
                )
                .unwrap()
            );
            let coder = &agents(&connection, SESSION).unwrap()[0];
            assert_eq!(coder.state_kind, None);
            assert_eq!(coder.state.as_deref(), Some(screen_state));
        }
    }

    #[test]
    fn a_kindless_wait_keeps_its_kind_until_the_state_changes() {
        let connection = seed();
        set_state(
            &connection,
            SESSION,
            CODER,
            "blocked",
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
            "blocked",
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
            "blocked",
            "hook",
            None,
            None,
            103,
        )
        .unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].state_kind, None);
        {
            let connection = seed();
            set_state(
                &connection,
                SESSION,
                CODER,
                "blocked",
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
    }

    #[test]
    fn a_screen_write_does_not_overwrite_a_newer_hook_report() {
        let connection = seed();
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
            "blocked",
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
        assert_eq!(coder.state.as_deref(), Some("blocked"));
        assert_eq!(coder.state_source.as_deref(), Some("hook"));
        let latest = (Some("blocked"), Some("hook"), Some(105), None);
        assert!(set_screen_state(&connection, SESSION, CODER, "done", None, 111, latest).unwrap());
        let unset = (None, None, None, None);
        assert!(
            set_screen_state(&connection, SESSION, ORCHESTRATOR, "done", None, 112, unset).unwrap()
        );
        {
            let connection = seed();
            set_state(
                &connection,
                SESSION,
                CODER,
                "blocked",
                "hook",
                None,
                Some("permission"),
                100,
            )
            .unwrap();
            let read = (Some("blocked"), Some("hook"), Some(100), Some("permission"));
            set_state(
                &connection,
                SESSION,
                CODER,
                "blocked",
                "hook",
                None,
                Some("question"),
                100,
            )
            .unwrap();
            assert!(
                !set_screen_state(&connection, SESSION, CODER, "done", None, 100, read).unwrap()
            );
            let coder = &agents(&connection, SESSION).unwrap()[0];
            assert_eq!(coder.state.as_deref(), Some("blocked"));
            assert_eq!(coder.state_kind.as_deref(), Some("question"));
        }
        {
            let connection = seed();
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
                "blocked",
                "hook",
                None,
                None,
                100,
            )
            .unwrap();
            let read = (Some("working"), Some("screen"), Some(100), None);
            assert!(
                !set_screen_state(&connection, SESSION, CODER, "done", None, 100, read).unwrap()
            );
            let coder = agents(&connection, SESSION)
                .unwrap()
                .into_iter()
                .find(|row| row.id == CODER)
                .unwrap();
            assert_eq!(coder.state.as_deref(), Some("blocked"));
        }
    }

    #[test]
    fn closing_a_pane_clears_the_agents_state() {
        let connection = seed();
        set_pane(&connection, SESSION, CODER, "%2").unwrap();
        set_state(
            &connection,
            SESSION,
            CODER,
            "blocked",
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
