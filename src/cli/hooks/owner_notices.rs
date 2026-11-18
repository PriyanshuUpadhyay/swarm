//! Owner notices: the rows a state change adds, the helper that posts them, and the rows due.

use super::*;
use swarm::store::notice_kind;

/// A turn shorter than this posts no done notice, because the owner most likely watched it
/// (ADR 0147).
const SHORT_TURN_S: i64 = 30;

/// ADR 0138: the chair's turn ends with a result or a question in plain text, so the owner gets a
/// done notice. A child's turn end gets none. While a child still works, the chair only waits for
/// its reply, so that turn end gets none either. ADR 0147: the row names the chat and quotes the
/// first sentence of the reply, and a reply that ends with a question is an `asks` row.
pub(crate) fn notice_chair_done(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    reply: Option<&str>,
    now: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    if !is_chair(connection, session, agent) {
        return Ok(());
    }

    if swarm::store::agents(connection, session)?
        .iter()
        .any(|child| child.id != agent && child.state.as_deref() == Some("working"))
    {
        return Ok(());
    }

    // The turn start is read after this report stored `done`, which keeps `turn_at`.
    if swarm::store::turn_at(connection, session, agent)?
        .is_some_and(|start| now - start < SHORT_TURN_S)
    {
        return Ok(());
    }

    let log = agent_log(connection, session, agent)?;
    let reply = reply
        .map(str::to_owned)
        .or_else(|| log.as_deref().and_then(swarm::view::discovery::last_reply));
    let kind = if reply.as_deref().is_some_and(ends_with_question) {
        notice_kind::ASKS
    } else {
        notice_kind::DONE
    };
    let body = reply
        .as_deref()
        .and_then(first_sentence)
        .unwrap_or_else(|| "The orchestrator is done.".into());
    add_chat_notice(connection, session, agent, log.as_deref(), kind, &body)
}

/// ADR 0147: the chair's turn ended in an error, such as a rate limit, so the owner gets the reason.
/// A short turn posts too, because the owner has to act on it.
pub(crate) fn notice_failed(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    detail: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !is_chair(connection, session, agent) {
        return Ok(());
    }
    let body = detail
        .and_then(first_sentence)
        .unwrap_or_else(|| "The turn stopped with an error.".into());
    let log = agent_log(connection, session, agent)?;
    add_chat_notice(
        connection,
        session,
        agent,
        log.as_deref(),
        notice_kind::FAILED,
        &body,
    )
}

/// ADR 0077: a change to blocked is a notice row, which the notice helper posts.
pub(crate) fn notice_blocked(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    kind: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let what = match kind {
        Some("permission") => "Blocked on a permission.",
        Some("question") => "Asked a question.",
        Some("auth") => "Blocked on a sign-in.",
        _ => "Blocked on a permission or a question.",
    };
    let log = agent_log(connection, session, agent)?;
    add_chat_notice(
        connection,
        session,
        agent,
        log.as_deref(),
        notice_kind::WAITING,
        what,
    )
}

fn is_chair(connection: &rusqlite::Connection, session: &str, agent: &str) -> bool {
    swarm::store::orchestrator_of(connection, session)
        .ok()
        .as_deref()
        == Some(agent)
}

/// The agent's log, else the chair's.
fn agent_log(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let agents = swarm::store::agents(connection, session)?;
    let chair = swarm::store::orchestrator_of(connection, session).ok();
    let log = |id: &str| {
        agents
            .iter()
            .find(|row| row.id == id)
            .and_then(|row| row.log.clone())
    };
    Ok(log(agent).or_else(|| chair.as_deref().and_then(log)))
}

/// A notice row titled with the chat's title (ADR 0147), then the helper start.
fn add_chat_notice(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    log: Option<&str>,
    kind: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let title = chat_title(connection, session, agent, log)?;
    swarm::store::add_notice(
        connection,
        Some(session),
        Some(agent),
        Some(kind),
        &title,
        body,
    )?;
    start_notify_helper();
    Ok(())
}

/// The chat's title from the chair's log, else from `log`, else the workspace folder. A child's
/// notice takes the chair's title, because the chair's log names the chat.
pub(crate) fn chat_title(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    log: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let chair = swarm::store::orchestrator_of(connection, session).ok();
    let chat = match chair {
        Some(chair) if chair != agent => agent_log(connection, session, &chair)?,
        _ => None,
    }
    .or_else(|| log.map(str::to_owned));

    Ok(chat
        .as_deref()
        .and_then(swarm::view::discovery::read_title)
        .or_else(|| {
            swarm::store::session_cwd(connection, session)
                .ok()
                .flatten()
                .map(|cwd| swarm::view::tree::basename(&cwd))
        })
        .unwrap_or_else(|| "Swarm".into()))
}

/// The reply's last line with words ends with a question mark.
fn ends_with_question(reply: &str) -> bool {
    reply
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.trim_end_matches(['*', '_', '`', ')']).ends_with('?'))
}

/// The most chars of a reply that a notice quotes.
const NOTICE_BODY_CHARS: usize = 160;

/// The first sentence of the first line with words, without Markdown marks or a list number, at
/// most `NOTICE_BODY_CHARS`. A dot after a short abbreviation ends no sentence.
fn first_sentence(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '>', '-', '*', ' '])
                .replace("**", "")
                .replace('`', "")
        })
        .find(|line| !line.trim().is_empty())?;
    let line = strip_list_number(line.trim());

    let end = line
        .char_indices()
        .find(|&(at, c)| {
            let after = &line[at + c.len_utf8()..];
            matches!(c, '.' | '?' | '!')
                && after.chars().next().is_none_or(char::is_whitespace)
                && !(c == '.' && after.trim_start().starts_with(char::is_lowercase))
                && !(c == '.' && is_abbreviation(&line[..at]))
        })
        .map_or(line.len(), |(at, c)| at + c.len_utf8());

    Some(swarm::view::tree::shorten(&line[..end], NOTICE_BODY_CHARS))
}

/// `line` without a leading `1.` or `2)` list number.
fn strip_list_number(line: &str) -> &str {
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    match line[digits..].strip_prefix(['.', ')']) {
        Some(rest) if digits > 0 && rest.starts_with(' ') => rest.trim_start(),
        _ => line,
    }
}

/// The word before a dot is an abbreviation: single letters with dots, such as "e.g" or "U.S", or
/// a short word such as "vs". A word with a longer part, such as the file name "main.rs" or a
/// version, is no abbreviation; a one-letter name such as "a.c" reads as one.
fn is_abbreviation(before: &str) -> bool {
    let word = before.rsplit(char::is_whitespace).next().unwrap_or("");
    // An opening bracket or quote, as in "(e.g", is not part of the word.
    let word = word.trim_start_matches(['(', '[', '{', '"', '\'', '“', '‘']);
    let letters = word.contains('.')
        && word
            .split('.')
            .all(|part| part.chars().count() == 1 && part.chars().all(char::is_alphabetic));
    letters || ["vs", "etc", "approx", "cf", "Mr", "Ms", "Dr"].contains(&word)
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
    // The login item in this HOME, so another home, such as a test's, keeps its banners.
    let hud_runs = env::var_os("HOME").is_some_and(|home| {
        let status = swarm::hud::status(std::path::Path::new(&home));
        status.installed && status.running
    });
    print_json(&due_notices(&mut connection, &root, hud_runs)?)
}

/// ADR 0147: while the HUD runs, it shows each chat's notices, so a row of a chat posts no banner.
/// A row with no chat, such as the Settings test notice or a `swarm notify` from a shell, has no
/// place in the HUD, so it keeps its banner.
pub(crate) fn due_notices(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    hud_runs: bool,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let rows = swarm::store::take_notices(connection)?;
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|row| !hud_runs || row.session_id.is_none())
        .collect();
    let connection = &*connection;

    // A bad choices file must not lose notices, so it reads as the defaults.
    let prefs = swarm::view::choices_store::load(root, true)
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
        let cwd = session.map(|id| swarm::store::session_cwd(connection, id));
        if let Some(cwd) = cwd.transpose()?.flatten()
            && muted
                .iter()
                .any(|project| swarm::view::tree::contains(&cwd, project))
        {
            continue;
        }

        let waiting = match (session, row.agent_id.as_deref()) {
            (Some(session), Some(agent)) => swarm::store::agents(connection, session)?
                .iter()
                .any(|found| found.id == agent && found.state.as_deref() == Some("blocked")),
            _ => false,
        };

        // The done switch mutes every other row: a question or a failure needs the owner, and the
        // Settings test notice checks macOS delivery.
        let must_post = matches!(
            row.kind.as_deref(),
            Some(notice_kind::ASKS | notice_kind::FAILED | notice_kind::TEST)
        );
        if waiting || must_post || on("done") {
            due.push(serde_json::json!({
                "session_id": row.session_id,
                "agent_id": row.agent_id,
                "title": row.title,
                "body": row.body,
                "sound": on("sound"),
            }));
        }
    }
    Ok(due)
}
