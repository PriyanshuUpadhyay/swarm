//! Owner notices: the rows a state change adds, the helper that posts them, and the rows due.

use super::*;

/// ADR 0138: the chair's turn ends with a result or a question in plain text, so the owner gets a
/// done notice. A child's turn end gets none. While a child still works, the chair only waits for
/// its reply, so that turn end gets none either.
pub(crate) fn notice_chair_done(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if swarm::store::orchestrator_of(connection, session)
        .ok()
        .as_deref()
        != Some(agent)
    {
        return Ok(());
    }
    if swarm::store::agents(connection, session)?
        .iter()
        .any(|child| child.id != agent && child.state.as_deref() == Some("working"))
    {
        return Ok(());
    }
    add_folder_notice(connection, session, agent, "the orchestrator is done")
}

/// ADR 0077: a change to waiting is a notice row, which the notice helper posts.
pub(crate) fn notice_waiting(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    kind: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let what = match kind {
        Some("permission") => "waiting on a permission",
        Some("question") => "asked a question",
        _ => "waiting on a permission or a question",
    };
    add_folder_notice(connection, session, agent, what)
}

/// A notice row titled Swarm whose body is `<workspace folder>: <what>`, then the helper start.
fn add_folder_notice(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    what: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let folder = swarm::store::session_cwd(connection, session)?
        .map(|cwd| swarm::view::tree::basename(&cwd))
        .unwrap_or_default();
    swarm::store::add_notice(
        connection,
        Some(session),
        Some(agent),
        "Swarm",
        &format!("{folder}: {what}"),
    )?;
    start_notify_helper();
    Ok(())
}

/// ADR 0077: a notice starts the small helper, never the full app. Never waits, so the hook keeps
/// its remaining budget. Reports a failure to start on stderr.
pub(crate) fn start_notify_helper() {
    use std::os::unix::process::CommandExt;
    let Some(helper) = notify_helper() else {
        return;
    };
    let mut command = std::process::Command::new("open");
    command.arg("-g").arg(helper);
    // The helper drains with this swarm and this home, whichever build LaunchServices starts.
    if let Ok(home) = swarm::paths::home() {
        command.arg("--env").arg(format!("SWARM_HOME={home}"));
    }
    if let Ok(exe) = std::env::current_exe() {
        command
            .arg("--env")
            .arg(format!("SWARM_BIN={}", exe.display()));
    }
    if let Err(error) = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
    {
        eprintln!("swarm: notice helper did not start: {error}");
    }
}

/// `SWARM_NOTIFY_APP` alone when set, else the helper that ships with this swarm (ADR 0079). Never
/// a bundle id: LaunchServices can pick a dev build.
pub(crate) fn notify_helper() -> Option<std::path::PathBuf> {
    if let Some(path) = env::var_os("SWARM_NOTIFY_APP") {
        return Some(std::path::PathBuf::from(path)).filter(|path| path.exists());
    }
    swarm::paths::bundled("SwarmNotify.app")
}

/// ADR 0077: take every unposted notice and print the ones the owner's notice preferences post,
/// each with its sound choice. A row whose agent is not waiting is a done notice.
pub(crate) fn notices_due() -> Result<(), Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let mut connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let rows = swarm::store::take_notices(&mut connection)?;
    // A bad choices file must not lose notices, so it reads as the defaults.
    let prefs = swarm::view::choices_store::load(&root, true)
        .map(|saved| saved.owner.prefs["notices"].clone())
        .unwrap_or_default();
    let on = |key: &str| prefs[key].as_bool().unwrap_or(true);
    let muted: Vec<&str> = prefs["mutedProjects"]
        .as_array()
        .map(|paths| paths.iter().filter_map(|path| path.as_str()).collect())
        .unwrap_or_default();
    let mut due = Vec::new();
    for row in rows.iter().filter(|_| on("post")) {
        let session = row.session_id.as_deref();
        // ponytail: a muted project matches a session folder inside it; a worktree kept outside
        // its project folder is not muted until this reads the tree.
        let cwd = session.map(|id| swarm::store::session_cwd(&connection, id));
        if let Some(cwd) = cwd.transpose()?.flatten()
            && muted
                .iter()
                .any(|project| swarm::view::tree::contains(&cwd, project))
        {
            continue;
        }
        let waiting = match (session, row.agent_id.as_deref()) {
            (Some(session), Some(agent)) => swarm::store::agents(&connection, session)?
                .iter()
                .any(|found| found.id == agent && found.state.as_deref() == Some("waiting")),
            _ => false,
        };
        // A test notice from Settings checks macOS delivery, so the done switch does not drop it.
        if waiting || on("done") || row.title == TEST_NOTICE_TITLE {
            due.push(serde_json::json!({
                "session_id": row.session_id,
                "agent_id": row.agent_id,
                "title": row.title,
                "body": row.body,
                "sound": on("sound"),
            }));
        }
    }
    print_json(&due)
}
