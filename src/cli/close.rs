//! `swarm close`: ask each child for a wrap-up, wait for its `finish`, save it, then close the pane.

use super::*;

/// The lines a reply to a wrap-up must hold, and what goes under each (ADR 0108).
const WRAP_UP: [(&str, &str); 5] = [
    ("## Done", "What you did, with file paths and commits."),
    ("## Why", "Why you did it that way."),
    (
        "## Decisions",
        "The choices you made, and what you rejected.",
    ),
    ("## Next steps", "What is left to do."),
    ("## Open questions", "What is not resolved."),
];

/// How long `close` waits for the wrap-ups of all the children it closes.
pub(crate) const WRAP_UP_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// Close the panes of `ids`. With `wait`, first send each child a wrap-up, and save each reply that
/// comes in time to `runs/<session>/<id>.summary.md`.
pub(crate) fn close_agents(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    ids: &[String],
    wait: Option<std::time::Duration>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Every pane is checked first, so a wrong id closes nothing.
    let mut panes: Vec<(&str, String)> = Vec::new();
    for id in ids {
        if !panes.iter().any(|(seen, _)| seen == id) {
            panes.push((id, swarm::store::require_pane(connection, session_id, id)?));
        }
    }
    let adapter = swarm::adapter::load(root, adapter_name)?;
    // A failure for one child is reported at the end, so it does not leave the others open.
    let mut failed = match wait {
        Some(wait) => wrap_up(connection, root, &adapter, session_id, &panes, wait)?,
        None => Vec::new(),
    };
    for (id, pane) in panes {
        // The `exited` hook clears the pane id but can leave the pane, so close it anyway; a pane
        // that is already gone is not a failure.
        match adapter.run("close", &[("pane", &pane)]) {
            Ok(_) => swarm::store::clear_pane(connection, session_id, id)?,
            Err(_) if swarm::store::pane_of(connection, session_id, id)?.is_none() => {}
            Err(error) => failed.push(format!("{id}: not closed: {error}")),
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    Err(format!("swarm: {}", failed.join("; swarm: ")).into())
}

/// How long `close` waits for the screen of a child that sent no wrap-up. A test's fake capture can
/// start slowly on a loaded machine, so tests wait longer.
const CAPTURE_WAIT: std::time::Duration =
    std::time::Duration::from_secs(if cfg!(test) { 15 } else { 5 });

/// How long all those screen reads may take together, so `close` stays inside the 180 s call
/// timeout that the host contracts name, for any number of silent children.
const CAPTURES_WAIT: std::time::Duration =
    std::time::Duration::from_secs(if cfg!(test) { 18 } else { 15 });

/// How often `close` looks for the wrap-ups.
const WRAP_UP_POLL: std::time::Duration = std::time::Duration::from_millis(200);

fn wrap_up(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    panes: &[(&str, String)],
    wait: std::time::Duration,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let orchestrator = swarm::store::orchestrator_of(connection, session_id)?;
    let mut failed = Vec::new();
    let ask = format!(
        "You are being closed. Do no more work. Reply once with `swarm finish` and a body with \
         these headings, each on its own line with its text under it:\n\n{}\n\
         Write \"none\" under a heading with nothing to say.",
        WRAP_UP
            .map(|(heading, what)| format!("{heading}\n{what}\n\n"))
            .concat()
    );
    let mut waiting = Vec::new();
    for (id, pane) in panes {
        if *id != orchestrator {
            // No ring waits for a turn, so all asks go out at once and the wait below is the bound.
            match deliver_with(
                connection,
                root,
                &adapter.name,
                session_id,
                &orchestrator,
                id,
                "wrap-up",
                &ask,
                Proof::Later,
            ) {
                Ok(seq) => waiting.push((*id, pane.as_str(), seq)),
                Err(error) => failed.push(format!("{id}: wrap-up not asked: {error}")),
            }
        }
    }
    let deadline = std::time::Instant::now() + wait;
    // The last look runs at the deadline, so a reply that comes during the final poll is kept.
    loop {
        let mut left = Vec::new();
        for (id, pane, seq) in waiting {
            // Read the pane before the replies, so a child that replies and then exits is saved.
            let alive = swarm::store::pane_of(connection, session_id, id)?.is_some();
            // Only a reply with every heading is the wrap-up; a death note is a summary too.
            let reply = swarm::store::summaries_after(connection, session_id, id, seq)?
                .into_iter()
                .filter_map(|body_path| std::fs::read_to_string(root.join(body_path)).ok())
                .find(|body| missing_headings(body).is_empty());
            if let Some(body) = reply {
                let saved = root.join(format!("runs/{session_id}/{id}.summary.md"));
                match swarm::store::write_atomic(&saved, &body) {
                    Ok(()) => println!("{}", saved.display()),
                    Err(error) => failed.push(format!("{id}: wrap-up not saved: {error}")),
                }
            } else if alive {
                left.push((id, pane, seq));
            }
        }
        waiting = left;
        if waiting.is_empty() || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(WRAP_UP_POLL);
    }
    let captures_end = std::time::Instant::now() + CAPTURES_WAIT;
    for (id, pane, _) in waiting {
        let log = root.join(format!("runs/{session_id}/{id}.log"));
        let left = captures_end.saturating_duration_since(std::time::Instant::now());
        let note = match adapter
            .capture_within(&[("pane", pane)], CAPTURE_WAIT.min(left))
            .map(|text| swarm::store::write_atomic(&log, &text))
        {
            Some(Ok(())) => format!(
                "agent {id} closed without a wrap-up; screen log: {}",
                log.display()
            ),
            Some(Err(error)) => {
                format!("agent {id} closed without a wrap-up; screen not saved: {error}")
            }
            None => format!("agent {id} closed without a wrap-up; screen not read"),
        };
        eprintln!("swarm: {note}");
        // The orchestrator's pane runs this command, so its turn cannot start until it returns.
        if let Err(error) = deliver_with(
            connection,
            root,
            &adapter.name,
            session_id,
            id,
            &orchestrator,
            "summary",
            &note,
            Proof::Later,
        ) {
            failed.push(format!("{id}: note not sent: {error}"));
        }
    }
    Ok(failed)
}

/// Refuse a `finish` body sent while `agent` owes a wrap-up, when the body misses a heading.
pub(crate) fn check_wrap_up_reply(
    connection: &rusqlite::Connection,
    session_id: &str,
    agent: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !swarm::store::owes_wrap_up(connection, session_id, agent)? {
        return Ok(());
    }
    let missing = missing_headings(body);
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "swarm: a wrap-up reply needs the line{} {}; write \"none\" under a heading with nothing to say",
        if missing.len() == 1 { "" } else { "s" },
        missing.join(", ")
    )
    .into())
}

fn missing_headings(body: &str) -> Vec<&'static str> {
    WRAP_UP
        .into_iter()
        .map(|(heading, _)| heading)
        .filter(|heading| !body.lines().any(|line| line.trim_end() == *heading))
        .collect()
}

#[cfg(test)]
mod tests;
