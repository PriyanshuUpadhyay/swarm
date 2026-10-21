//! `swarm close`: ask each child for a wrap-up, wait for its `finish`, save it, then close the pane.

/// The lines a reply to a wrap-up must hold (ADR 0108).
const WRAP_UP_HEADINGS: [&str; 5] = [
    "## Done",
    "## Why",
    "## Decisions",
    "## Next steps",
    "## Open questions",
];

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
    let missing: Vec<&str> = WRAP_UP_HEADINGS
        .into_iter()
        .filter(|heading| !body.lines().any(|line| line.trim_end() == *heading))
        .collect();
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
        let full = WRAP_UP_HEADINGS.map(|h| format!("{h}\nnone\n")).concat();
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
}
