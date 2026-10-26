//! Board pings (ADR 0118): which post rings whom, as a message row in the post's transaction.

use super::*;

/// An ask chain deeper than this rings no peer, so two seats cannot keep asking each other.
const MAX_DEPTH: i64 = 3;
/// The paneless sender of every board ping, made in each session that gets one.
pub const BOARD_SENDER: &str = "board";

/// Stores the messages that post B-n rings. The text is fixed words, agent ids, and the post id,
/// never agent text, so a ping cannot carry an instruction. Board rows skip the talk-mode route,
/// because the refusals above already gate them.
#[allow(clippy::too_many_arguments)]
pub(super) fn board_pings(
    tx: &Connection,
    root: &Path,
    project_id: &str,
    session_id: &str,
    author: &str,
    n: i64,
    new: &NewPost,
    depth: i64,
) -> Result<Vec<Ping>, Box<dyn std::error::Error>> {
    let line = |what: String| format!("board post B-{n}: {what}. Run swarm board read B-{n}.");
    // Only a ping to the lead needs one, so a post that rings nobody works in a seat-only session.
    let lead = || orchestrator_of(tx, session_id);
    let mut targets: Vec<(String, String, &str, String)> = Vec::new();
    match new.kind {
        "ask" if depth > MAX_DEPTH => {
            // ponytail: one alert per branch that crosses the cap, not per root; a forked chain alerts per fork.
            if depth == MAX_DEPTH + 1 {
                let what =
                    format!("ask from {author} is past depth {MAX_DEPTH}, so no peer was rung");
                targets.push((session_id.into(), lead()?, "depth", line(what)));
            }
        }
        "ask" | "handoff" => {
            if let Some((to_session, to_agent)) = new.to {
                let what = format!("{} from {author}", new.kind);
                targets.push((to_session.into(), to_agent.into(), new.kind, line(what)));
            }
        }
        "answer" => {
            let ask = new.reply_to.ok_or("an answer has no ask")?;
            let (asker_session, asker): (String, String) = tx.query_row(
                "SELECT session_id, author FROM post WHERE project_id = ?1 AND n = ?2",
                (project_id, ask),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let what = format!("answer from {author} to your ask B-{ask}");
            targets.push((asker_session, asker, "answer", line(what)));
        }
        "decision" if new.needs_owner => {
            let what = format!("decision from {author} waits for the owner");
            targets.push((session_id.into(), lead()?, "owner", line(what)));
            // The binary writes the notice, so a worker that cannot run `swarm notify` still reaches the owner.
            add_notice(
                tx,
                Some(session_id),
                Some(author),
                "swarm: a decision waits for you",
                &format!("B-{n}: {}", new.subject.trim()),
            )?;
        }
        _ => {}
    }
    let mut pings = Vec::new();
    for (to_session, to_agent, kind, text) in targets {
        if to_session == session_id && to_agent == author {
            continue;
        }
        board_sender(tx, &to_session)?;
        let kind = format!("board:{kind}");
        let seq = insert_message(tx, root, &to_session, BOARD_SENDER, &to_agent, &kind, &text)?;
        pings.push(Ping {
            session_id: to_session,
            agent_id: to_agent,
            seq,
        });
    }
    Ok(pings)
}

fn board_sender(tx: &Connection, session_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let role: Option<String> = tx
        .query_row(
            "SELECT role FROM agent WHERE session_id = ?1 AND id = ?2",
            (session_id, BOARD_SENDER),
            |r| r.get(0),
        )
        .optional()?;
    match role.as_deref() {
        None => {
            tx.execute(
                "INSERT INTO agent (id, session_id, role) VALUES (?1, ?2, ?1)",
                (BOARD_SENDER, session_id),
            )?;
            Ok(())
        }
        Some(BOARD_SENDER) => Ok(()),
        Some(_) => Err(format!(
            "swarm: session {session_id} has a seat named {BOARD_SENDER}; rename it so board pings can be sent"
        )
        .into()),
    }
}
