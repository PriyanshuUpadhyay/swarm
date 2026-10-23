//! CLI delivery.

use super::*;

/// Refuse a sender or recipient that is not an agent in the session, and a recipient without a
/// pane, then store the message and ring it. The bell is a hint (R9), so a ring failure only warns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn deliver(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    sender: &str,
    recipient: &str,
    kind: &str,
    body: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    deliver_with(
        connection,
        root,
        adapter_name,
        session_id,
        sender,
        recipient,
        kind,
        body,
        Proof::Wait,
    )
}

/// `deliver`, with the ring's proof wait chosen by the caller.
#[allow(clippy::too_many_arguments)]
pub(crate) fn deliver_with(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    sender: &str,
    recipient: &str,
    kind: &str,
    body: &str,
    proof: Proof,
) -> Result<i64, Box<dyn std::error::Error>> {
    let agents = swarm::store::agents(connection, session_id)?;
    let agent = |id: &str| {
        agents
            .iter()
            .find(|agent| agent.id == id)
            .ok_or_else(|| format!("swarm: {id} is not an agent in session {session_id}"))
    };
    agent(sender)?;
    let (recipient, kind) = swarm::store::route(connection, session_id, sender, recipient, kind)?;
    let recipient_row = agent(&recipient)?;
    let pane = recipient_row
        .pane
        .clone()
        .ok_or_else(|| format!("swarm: {recipient} has no pane; nothing would ring it"))?;
    let seq = swarm::store::send_message(
        connection, root, session_id, sender, &recipient, &kind, body,
    )?;
    ring_stored(
        connection,
        root,
        adapter_name,
        session_id,
        &recipient,
        &pane,
        seq,
        proof,
    );
    Ok(seq)
}

/// Rings a stored message unless the recipient already has a rung unread one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ring_stored(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    recipient: &str,
    pane: &str,
    seq: i64,
    proof: Proof,
) {
    // The message is stored now, so a failed ring mark only warns: the row stays unrung, a later
    // re-ring rings it, and a caller such as `report` does not send it a second time.
    let mark = || -> Result<Option<i64>, Box<dyn std::error::Error>> {
        if swarm::store::has_rung_unread(connection, session_id, recipient)? {
            return Ok(None);
        }
        let rung_at = unix_now()?;
        connection.execute(
            "UPDATE message SET rung_at = ?3, rings = 1 WHERE session_id = ?1 AND seq = ?2",
            (session_id, seq, rung_at),
        )?;
        Ok(Some(rung_at))
    };
    match mark() {
        Ok(Some(rung_at)) => {
            ring_and_record(
                connection,
                root,
                swarm::adapter::load(root, adapter_name),
                session_id,
                recipient,
                pane,
                &[seq],
                rung_at,
                proof,
            );
        }
        Ok(None) => {}
        Err(error) => eprintln!("swarm: message {seq} stored but not rung: {error}"),
    }
}

pub(crate) fn ack(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    agent_id: &str,
    seq: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    swarm::store::ack(connection, session_id, seq, agent_id)?;
    if let Some(pane) = swarm::store::pane_of(connection, session_id, agent_id)?
        && swarm::store::has_unrung_unread(connection, session_id, agent_id)?
    {
        let rung_at = unix_now()?;
        let seqs: Vec<i64> = connection
            .prepare(
                "UPDATE message SET rung_at = ?3, rings = 1
                 WHERE session_id = ?1 AND recipient_id = ?2 AND rings = 0
                   AND NOT EXISTS (
                       SELECT 1 FROM read_mark
                       WHERE read_mark.session_id = message.session_id
                         AND message_seq = message.seq AND agent_id = ?2
                   )
                 RETURNING seq",
            )?
            .query_map((session_id, agent_id, rung_at), |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        ring_and_record(
            connection,
            root,
            swarm::adapter::load(root, adapter_name),
            session_id,
            agent_id,
            &pane,
            &seqs,
            rung_at,
            Proof::Wait,
        );
    }
    Ok(())
}

/// Settle each ring that no caller waited for: the listing's, and one whose caller ended in its
/// wait. Its proof is a hook, the screen, or a read now; with none, it is unconfirmed once its deadline has
/// passed (ADR 0041). Adds a line for the chair's own lost message to `lines` at once, so the line
/// stays when a later ring fails. With `Proof::Later`, the listing's pass, the chair's own last ring
/// stays for the sweep.
pub(crate) fn settle_rings(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    proof: Proof,
    lines: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let chair = swarm::store::orchestrator_of(connection, session_id)?;
    let now = unix_now()?;
    let text = ring_text(root);
    for (agent, rung_at, seqs, lost) in swarm::store::unsettled_rings(connection, session_id)? {
        // The app drops the listing's stderr, so the chair's own last ring is left to `swarm
        // sweep`, whose line can name it (ADR 0041).
        if proof == Proof::Later && agent == chair && lost.is_some() {
            continue;
        }
        let provider = swarm::store::provider_of(connection, session_id, &agent)?;
        let provider = provider.as_deref();
        // A failed capture hides only the screen: the store's hook and read still prove the ring,
        // and with neither only the deadline settles it.
        let (rows, screen) = match swarm::store::pane_of(connection, session_id, &agent)? {
            Some(pane) => {
                refresh_host(connection, adapter, session_id, &agent, provider, &pane);
                let vars = [("pane", pane.as_str())];
                adapter
                    .capture_within(&vars, std::time::Duration::from_secs(1))
                    .map(|rows| {
                        let screen = screen_of(adapter, &vars, &rows);
                        (rows, screen)
                    })
                    .unwrap_or_default()
            }
            // No pane is an empty screen: unchecked with no provider, else unconfirmed when late.
            None => (String::new(), Some(String::new())),
        };
        let held = provider.is_some_and(|provider| swarm::screen::holds(provider, &rows, &text));
        let proven = (!held)
            .then(|| {
                ring_proof(
                    connection,
                    session_id,
                    &agent,
                    provider,
                    &rows,
                    screen.as_deref(),
                    rung_at,
                )
            })
            .transpose()?
            .flatten();
        // A read that the batch deadline stopped is no evidence, so only the store's proof
        // settles the ring in this pass, and a pass with time to read applies the deadline.
        let late =
            adapter.check_deadline().is_ok() && now - rung_at > RING_TIMEOUT.as_secs() as i64;
        let Some(delivery) = proven.or(late.then_some(Delivery::Unconfirmed)) else {
            continue;
        };
        let stored =
            swarm::store::set_delivery(connection, session_id, &seqs, rung_at, delivery.as_str())?;
        if let (true, Delivery::Unconfirmed, Some(seq), true) =
            (stored, delivery, lost, agent == chair)
        {
            lines.push(chair_lost(&agent, seq));
        }
    }
    Ok(())
}

/// Send the chair `unconfirmed:<seq>` for each message whose last ring proved nothing and that no
/// report names yet (ADR 0041), and return a line for each one sent.
pub(crate) fn report_lost(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    proof: Proof,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let chair = swarm::store::orchestrator_of(connection, session_id)?;
    let mut lines = Vec::new();
    for (agent, seq) in swarm::store::unreported_lost(connection, session_id, &chair)? {
        if report(
            connection,
            root,
            adapter_name,
            session_id,
            &agent,
            &format!("unconfirmed:{seq}"),
            &lost_body(&agent, seq),
            proof,
        ) {
            lines.push(format!("unconfirmed {agent} {seq}"));
        }
    }
    Ok(lines)
}

/// Send the chair one message for each stall of a child that no report names yet (ADR 0041),
/// and return a line for each one sent.
pub(crate) fn report_stalls(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    proof: Proof,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut lines = Vec::new();
    for (agent, what, seq) in swarm::store::stalls(connection, session_id)? {
        let body = match what {
            swarm::store::StallKind::Unacked => {
                format!("{agent} is done but has not acked message {seq}")
            }
            swarm::store::StallKind::Silent => {
                format!("{agent} finished its turn after message {seq} and sent nothing back")
            }
        };
        let what = what.as_str();
        let kind = format!("stall:{what}:{seq}");
        if report(
            connection,
            root,
            adapter_name,
            session_id,
            &agent,
            &kind,
            &body,
            proof,
        ) {
            lines.push(format!("stall {agent} {what} {seq}"));
        }
    }
    Ok(lines)
}

#[cfg(test)]
#[path = "delivery/tests.rs"]
mod tests;
