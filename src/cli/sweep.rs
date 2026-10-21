//! CLI sweep.

use super::*;

/// A child ended without `swarm finish`: forget its pane, then send a fallback summary in its
/// name and queue a summarize job unless it already sent one (R19).
pub(crate) fn report_dead(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    child: &str,
    note: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    swarm::store::clear_pane(connection, session_id, child)?;
    if !swarm::store::answered(connection, session_id, child)? {
        let orchestrator = swarm::store::orchestrator_of(connection, session_id)?;
        deliver(
            connection,
            root,
            adapter_name,
            session_id,
            child,
            &orchestrator,
            "summary",
            note,
        )?;
    }
    Ok(())
}

/// Ring `agent` again when its unseen messages are due for another ring. When their last ring
/// starts no turn and `agent` is the chair, returns the report line `unconfirmed <agent> <seq>`; a
/// child's lost message is reported by `report_lost`.
pub(crate) fn rering_if_due(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent: &str,
    pane: &str,
    proof: Proof,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if !swarm::store::rering_due(connection, session_id, agent, RERING_UNSEEN_AFTER_SECS)? {
        return Ok(None);
    }
    // Read before the ring: once the ring stores `unconfirmed`, no pass finds it again, so an
    // error after it would drop the chair's line. Only a waiting ring has a result to report.
    let chair = match proof {
        Proof::Wait => Some(swarm::store::orchestrator_of(connection, session_id)?),
        Proof::Later => None,
    };
    let rung_at = unix_now()?;
    let rung = swarm::store::rering(
        connection,
        session_id,
        agent,
        RERING_UNSEEN_AFTER_SECS,
        rung_at,
    )?;
    // Another pass rang them since `rering_due` read the store.
    if rung.is_empty() {
        return Ok(None);
    }
    let seqs: Vec<i64> = rung.iter().map(|(seq, _)| *seq).collect();
    let delivery = ring_and_record(
        connection,
        root,
        Ok(adapter.clone()),
        session_id,
        agent,
        pane,
        &seqs,
        rung_at,
        proof,
    );
    eprintln!("swarm: re-ringed {agent}");
    let lost = rung
        .iter()
        .filter(|(_, rings)| *rings >= swarm::store::MAX_RINGS)
        .map(|(seq, _)| *seq)
        .min();
    match (delivery, lost) {
        (Some(Delivery::Unconfirmed), Some(seq)) if chair.as_deref() == Some(agent) => {
            Ok(Some(chair_lost(agent, seq)))
        }
        _ => Ok(None),
    }
}

/// The body of the report that message `seq` to `agent` was lost.
pub(crate) fn lost_body(agent: &str, seq: i64) -> String {
    format!(
        "message {seq} to {agent} was not delivered: no turn started after {} rings",
        swarm::store::MAX_RINGS
    )
}

/// A message about the chair would ring the pane that just lost two rings, so the chair's own lost
/// message is only stderr and this line (ADR 0041).
pub(crate) fn chair_lost(chair: &str, seq: i64) -> String {
    eprintln!("swarm: {}", lost_body(chair, seq));
    format!("unconfirmed {chair} {seq}")
}

/// Send `subject`'s report `kind` to the chair, once: the trigger `message_report` refuses a
/// second one. Returns whether this call stored it. A failed send only warns, so the pass that
/// found the report goes on, and the next pass finds the report again.
#[allow(clippy::too_many_arguments)]
pub(crate) fn report(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    subject: &str,
    kind: &str,
    body: &str,
    proof: Proof,
) -> bool {
    let sent = swarm::store::orchestrator_of(connection, session_id).and_then(|chair| {
        deliver_with(
            connection,
            root,
            adapter_name,
            session_id,
            subject,
            &chair,
            kind,
            body,
            proof,
        )
    });
    match sent {
        Ok(_) => true,
        Err(error) => {
            let reported = matches!(
                error.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::SqliteFailure(failure, _))
                    if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER
            );
            if !reported {
                eprintln!("swarm: report {kind} for {subject} not sent: {error}");
            }
            false
        }
    }
}

/// One sweep pass: re-ring unseen messages, the sweeper's own included, and report each child
/// whose pane is gone, each lost message, and each stall. Adds one line per report to `lines`, for
/// the sweep's output. A report is sent at once, so its line stays even when a later step fails.
pub(crate) fn sweep_once(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent_id: &str,
    lines: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // A child's ring to the chair can be lost too, and no other sweeper covers the chair.
    if let Some(pane) = swarm::store::pane_of(connection, session_id, agent_id)? {
        lines.extend(rering_if_due(
            connection,
            root,
            adapter,
            session_id,
            agent_id,
            &pane,
            Proof::Wait,
        )?);
    }
    for (child, pane) in swarm::store::live_children(connection, session_id, agent_id)? {
        if adapter.has_pane(&pane)? {
            lines.extend(rering_if_due(
                connection,
                root,
                adapter,
                session_id,
                &child,
                &pane,
                Proof::Wait,
            )?);
            continue;
        }
        let note = format!("agent {child} died without a summary");
        report_dead(connection, root, &adapter.name, session_id, &child, &note)?;
        lines.push(format!("dead {child}"));
    }
    settle_rings(connection, root, adapter, session_id, Proof::Wait, lines)?;
    // A report rings the chair, so like the listing's it waits for a pass that reads the chair idle.
    if !chair_idle(connection, adapter, session_id)? {
        return Ok(());
    }
    lines.extend(report_lost(
        connection,
        root,
        &adapter.name,
        session_id,
        Proof::Wait,
    )?);
    lines.extend(report_stalls(
        connection,
        root,
        &adapter.name,
        session_id,
        Proof::Wait,
    )?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn sweep_rerings_a_child_once_for_old_unseen_messages() {
        let root = std::env::temp_dir().join(format!("swarm-sweep-test-{}", std::process::id()));
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "one",
        )
        .unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "two",
        )
        .unwrap();
        connection
            .execute("UPDATE message SET created_at = unixepoch() - 61", [])
            .unwrap();

        let adapter = swarm::adapter::parse(
            "fake",
            &format!("self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = echo %2\nclose = true\ncapture = true\n", ring_log.display()),
        )
        .unwrap();
        swept(&mut connection, &root, &adapter, &session);
        swept(&mut connection, &root, &adapter, &session);
        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);
        connection
            .execute("UPDATE message SET rung_at = unixepoch() - 61", [])
            .unwrap();
        swept(&mut connection, &root, &adapter, &session);
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));

        swarm::store::inbox(&connection, &session, CODER).unwrap();
        connection
            .execute("UPDATE message SET rung_at = unixepoch() - 61", [])
            .unwrap();
        swept(&mut connection, &root, &adapter, &session);
        swarm::store::ack(&connection, &session, 1, CODER).unwrap();
        connection
            .execute(
                "UPDATE message SET seen_at = NULL, rung_at = unixepoch() - 61 WHERE seq = 1",
                [],
            )
            .unwrap();
        swept(&mut connection, &root, &adapter, &session);

        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), ring.repeat(2));
    }

    #[test]
    fn sweep_reports_a_dead_child_as_before() {
        let root = std::env::temp_dir().join(format!("swarm-dead-test-{}", std::process::id()));
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        // The dead-child report must ring through the sweep's adapter, never the real host's.
        std::fs::write(
            root.join("adapters/fake.conf"),
            format!(
                "self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
                ring_log.display()
            ),
        )
        .unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        let adapter = swarm::adapter::parse(
            "fake",
            "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();

        swept(&mut connection, &root, &adapter, &session);

        assert_eq!(
            swarm::store::pane_of(&connection, &session, CODER).unwrap(),
            None
        );
        assert!(swarm::store::answered(&connection, &session, CODER).unwrap());
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "%1\n");
    }

    /// `swarm sweep` sends the chair one message for each stall kind of a child, and no second
    /// one for the same stall (ADR 0041).
    #[test]
    fn sweep_reports_each_stall_to_the_chair_once() {
        let (root, mut connection, session) = ring_session(
            "stall",
            None,
            "ring = true",
            include_str!("../../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("claude", "chair-id"))).unwrap();
        let ask = send_task(&root, &mut connection, &session);
        // The ring started a turn half a minute ago; the coder read the task, then got done.
        connection
            .execute_batch(
                "UPDATE message SET delivery = 'hook', rung_at = unixepoch() - 30,
                                    seen_at = unixepoch() - 20",
            )
            .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "done",
            "hook",
            None,
            None,
            now,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let sweep =
            |connection: &mut rusqlite::Connection| swept(connection, &root, &adapter, &session);

        assert_eq!(
            sweep(&mut connection),
            [format!("stall coder unacked {ask}")]
        );
        assert!(sweep(&mut connection).is_empty());
        swarm::store::ack(&connection, &session, ask, CODER).unwrap();
        assert_eq!(
            sweep(&mut connection),
            [format!("stall coder silent {ask}")]
        );
        assert!(sweep(&mut connection).is_empty());
        let kinds: Vec<String> = chair_mail(&connection, &session)
            .into_iter()
            .map(|(_, kind)| kind)
            .collect();
        assert_eq!(
            kinds,
            [
                format!("stall:unacked:{ask}"),
                format!("stall:silent:{ask}")
            ]
        );
        let body = std::fs::read_to_string(root.join(format!("runs/{session}/1.txt"))).unwrap();
        assert_eq!(body, "coder is done but has not acked message 0");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sweep_rerings_its_own_chair_for_an_old_unseen_finish() {
        let root =
            std::env::temp_dir().join(format!("swarm-sweep-chair-test-{}", std::process::id()));
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection.execute("UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61, rings = 1", []).unwrap();
        let adapter = swarm::adapter::parse(
            "fake",
            &format!("self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE\" >> '{}'\nlist = echo %2\nclose = true\ncapture = true\n", ring_log.display()),
        )
        .unwrap();

        swept(&mut connection, &root, &adapter, &session);

        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "%1\n");
    }

    #[test]
    fn a_failed_dead_agent_report_still_clears_its_pane() {
        let root =
            std::env::temp_dir().join(format!("swarm-dead-report-test-{}", std::process::id()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();

        assert!(report_dead(&mut connection, &root, "fake", &session, CODER, "dead").is_err());
        assert_eq!(
            swarm::store::pane_of(&connection, &session, CODER).unwrap(),
            None
        );
    }
}
