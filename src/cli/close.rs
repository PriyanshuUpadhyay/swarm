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

/// How long `close` waits for the screen of a child that sent no wrap-up.
const CAPTURE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

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
    for (id, pane, _) in waiting {
        let log = root.join(format!("runs/{session_id}/{id}.log"));
        let note = match adapter
            .capture_within(&[("pane", pane)], CAPTURE_WAIT)
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
mod tests {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn a_wrap_up_reply_needs_every_heading() {
        let root = std::env::temp_dir().join(format!("swarm-wrap-up-{}", std::process::id()));
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
        let full = WRAP_UP.map(|(h, _)| format!("{h}\nnone\n")).concat();
        let check = |connection: &rusqlite::Connection, body: &str| {
            check_wrap_up_reply(connection, &session, CODER, body).map_err(|e| e.to_string())
        };

        assert!(check(&connection, "a plain reply").is_ok());
        let send = |connection: &mut rusqlite::Connection, sender, recipient, kind| {
            swarm::store::send_message(connection, &root, &session, sender, recipient, kind, "x")
                .unwrap();
        };
        send(&mut connection, ORCHESTRATOR, CODER, "wrap-up");
        let refused = check(&connection, &full.replace("## Why\n", "")).unwrap_err();
        assert!(refused.contains("needs the line ## Why;"), "{refused}");
        assert!(
            check(&connection, "a plain reply")
                .unwrap_err()
                .contains("## Done, ## Why")
        );
        assert!(check(&connection, &full).is_ok());
        send(&mut connection, CODER, ORCHESTRATOR, "summary");
        assert!(check(&connection, "a plain reply").is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A file store with the orchestrator on `%1` and the coder on `%2`, whose fake adapter's
    /// `capture` prints `screen text`.
    fn closing(name: &str) -> (std::path::PathBuf, rusqlite::Connection, String) {
        let root = std::env::temp_dir().join(format!("swarm-{name}-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        std::fs::write(
            root.join("adapters/fake.conf"),
            "self = true\nspawn = true\nring = true\nlist = echo %1 %2\nclose = true\n\
             capture = echo screen text\n",
        )
        .unwrap();
        let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        (root, connection, session)
    }

    /// A pane that will not close is reported, and the panes after it still close.
    #[test]
    fn a_failed_close_does_not_stop_the_others() {
        let (root, connection, session) = closing("close-failed");
        std::fs::write(
            root.join("adapters/fake.conf"),
            "self = true\nspawn = true\nring = true\nlist = echo %1 %2 %3\ncapture = true\nclose = test \"$SWARM_PANE\" != %2\n",
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, "tester", "tester").unwrap();
        swarm::store::set_pane(&connection, &session, "tester", "%3").unwrap();
        let mut connection = connection;
        let ids = [CODER.to_string(), "tester".to_string()];

        let error = close_agents(&mut connection, &root, "fake", &session, &ids, None).unwrap_err();

        assert!(error.to_string().contains("coder: not closed"), "{error}");
        assert_eq!(
            swarm::store::pane_of(&connection, &session, CODER).unwrap(),
            Some("%2".into())
        );
        assert_eq!(
            swarm::store::pane_of(&connection, &session, "tester").unwrap(),
            None
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn close(
        connection: &mut rusqlite::Connection,
        root: &std::path::Path,
        session: &str,
        wait: Option<u64>,
    ) {
        // The same id twice gets one ask and one close.
        let ids = [CODER.to_string(), CODER.to_string()];
        let wait = wait.map(std::time::Duration::from_secs);
        close_agents(connection, root, "fake", session, &ids, wait).unwrap();
        assert_eq!(
            swarm::store::pane_of(connection, session, CODER).unwrap(),
            None
        );
    }

    /// The child answers the wrap-up from its own process, as `swarm finish` does.
    #[test]
    fn close_saves_the_childs_wrap_up_then_closes_its_pane() {
        let (root, mut connection, session) = closing("close-wrap-up");
        let body = WRAP_UP.map(|(h, _)| format!("{h}\nnone\n")).concat();
        let child = {
            let (root, session, body) = (root.clone(), session.clone(), body.clone());
            std::thread::spawn(move || {
                let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !swarm::store::owes_wrap_up(&connection, &session, CODER).unwrap() {
                    assert!(std::time::Instant::now() < deadline, "no wrap-up was sent");
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                check_wrap_up_reply(&connection, &session, CODER, &body).unwrap();
                swarm::store::send_message(
                    &mut connection,
                    &root,
                    &session,
                    CODER,
                    ORCHESTRATOR,
                    "summary",
                    &body,
                )
                .unwrap();
            })
        };

        close(&mut connection, &root, &session, Some(30));

        child.join().unwrap();
        let asks = swarm::store::messages(&connection, &session, -1).unwrap();
        assert_eq!(asks.iter().filter(|m| m.kind == "wrap-up").count(), 1);
        let saved = root.join(format!("runs/{session}/{CODER}.summary.md"));
        assert_eq!(std::fs::read_to_string(saved).unwrap(), body);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_child_with_no_wrap_up_in_time_leaves_its_screen_and_a_note() {
        let (root, mut connection, session) = closing("close-silent");

        close(&mut connection, &root, &session, Some(1));

        let log = root.join(format!("runs/{session}/{CODER}.log"));
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "screen text\n");
        let inbox = swarm::store::inbox(&connection, &session, ORCHESTRATOR).unwrap();
        let note = std::fs::read_to_string(root.join(&inbox[0].body_path)).unwrap();
        assert_eq!(
            note,
            format!(
                "agent coder closed without a wrap-up; screen log: {}",
                log.display()
            )
        );
        assert!(swarm::store::answered(&connection, &session, CODER).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A child that dies during the wait gets the fallback note, which is not its wrap-up.
    #[test]
    fn a_death_note_is_not_saved_as_the_wrap_up() {
        let (root, mut connection, session) = closing("close-dead");
        let child = {
            let (root, session) = (root.clone(), session.clone());
            std::thread::spawn(move || {
                let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !swarm::store::owes_wrap_up(&connection, &session, CODER).unwrap() {
                    assert!(std::time::Instant::now() < deadline, "no wrap-up was sent");
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                report_dead(
                    &mut connection,
                    &root,
                    "fake",
                    &session,
                    CODER,
                    "agent coder died",
                )
                .unwrap();
            })
        };
        let started = std::time::Instant::now();

        close(&mut connection, &root, &session, Some(30));

        child.join().unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert!(
            !root
                .join(format!("runs/{session}/{CODER}.summary.md"))
                .exists()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn close_now_sends_no_wrap_up() {
        let (root, mut connection, session) = closing("close-now");

        close(&mut connection, &root, &session, None);

        assert!(
            swarm::store::messages(&connection, &session, -1)
                .unwrap()
                .is_empty()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
