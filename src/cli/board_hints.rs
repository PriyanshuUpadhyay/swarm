//! Board hints: one stderr line at an agent's first `inbox` and at its `finish` (ADR 0118).

use super::*;

/// The trial runs found facts that the next run needs, but no seat posted, because no step asked
/// it to. `swarm finish` is the step that every seat runs, so it asks a seat that wrote no post.
pub(crate) fn finish_hint(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
) -> Option<&'static str> {
    let cwd = swarm::store::session_cwd(connection, session).ok()??;
    project_of(&cwd).ok()?;
    if swarm::store::has_posted(connection, session, agent).unwrap_or(true) {
        return None;
    }
    Some(
        "swarm: you wrote no board post in this session. Post each fact that the next run in this \
         project needs, with `swarm board post finding --subject <text> --source <path|url>` and \
         the body on stdin. Skip it if the run found nothing new.",
    )
}

/// The trial runs read the board at most at the end, so no run used an earlier run's post.
/// `swarm inbox` is the first step of every agent, so its first call names the board when the
/// reader can see a current post.
pub(crate) fn start_hint(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
) -> Option<String> {
    let cwd = swarm::store::session_cwd(connection, session).ok()??;
    let project = project_of(&cwd).ok()?;
    if swarm::store::has_acked(connection, session, agent).unwrap_or(true) {
        return None;
    }
    let posts = swarm::store::posts_for(connection, &project, Some((session, agent))).ok()?;
    let current: Vec<_> = posts.iter().filter(|p| p.superseded_by.is_none()).collect();
    let newest = current.last()?.n;
    Some(format!(
        "swarm: this project's board has {} current posts, newest B-{newest}. Run `swarm board \
         read` before you start, and use what applies to your task.",
        current.len()
    ))
}
