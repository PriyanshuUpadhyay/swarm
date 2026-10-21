//! CLI hooks.

use super::*;

/// What one hook call reports: the session and agent, the state with its detail and wait kind,
/// and the chat log path from the payload.
#[derive(Debug, PartialEq)]
pub(crate) struct HookReport {
    pub(crate) session: String,
    pub(crate) agent: String,
    pub(crate) state: Option<swarm::host::HookState>,
    pub(crate) log: Option<std::path::PathBuf>,
    pub(crate) usage_provider: Option<String>,
}

/// The chat log a hook payload names, only when it is an absolute path to an existing `.jsonl`
/// file. Claude and Codex send `transcript_path`, which Codex may leave null; AGY sends
/// `transcriptPath`.
pub(crate) fn hook_log(payload: &serde_json::Value) -> Option<std::path::PathBuf> {
    let path = ["transcript_path", "transcriptPath"]
        .iter()
        .find_map(|name| payload.get(*name)?.as_str())
        .map(std::path::PathBuf::from)?;
    (path.is_absolute()
        && path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        && path.is_file())
    .then_some(path)
}

/// What one hook call reports, or None when the caller is no swarm agent or the event says
/// nothing about state and names no chat log. `args` is `<provider> [event]`; AGY sends no event
/// name in its payload, so its hook command names the event.
pub(crate) fn hook_report(
    args: &[String],
    payload: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Option<HookReport>, Box<dyn std::error::Error>> {
    let (provider, event) = match args {
        [provider] => (provider, None),
        [provider, event] => (provider, Some(event.as_str())),
        _ => return Err(USAGE.into()),
    };
    if Provider::parse(provider).is_none() {
        return Err(USAGE.into());
    }
    let (Some(session), Some(agent)) = (env("SWARM_SESSION_ID"), env("SWARM_AGENT_ID")) else {
        return Ok(None);
    };
    let payload: serde_json::Value = match payload.trim() {
        "" => serde_json::json!({}),
        text => serde_json::from_str(text)?,
    };
    let event = event
        .or_else(|| payload.get("hook_event_name")?.as_str())
        .unwrap_or_default();
    let state = swarm::host::hook_state(provider, event, &payload);
    let log = hook_log(&payload);
    if state.is_none() && log.is_none() {
        return Ok(None);
    }
    let usage_provider =
        (event == "Stop" && state.is_some() && matches!(provider.as_str(), "claude" | "codex"))
            .then(|| provider.clone());
    Ok(Some(HookReport {
        session: valid_session_id(&session)?,
        agent,
        state,
        log,
        usage_provider,
    }))
}

/// All of `reader` if it closes within `limit`, else None. The read runs on its own thread, so
/// a writer that keeps the pipe open cannot hold the caller past the limit.
pub(crate) fn read_within<R: std::io::Read + Send + 'static>(
    mut reader: R,
    limit: std::time::Duration,
) -> Option<String> {
    let (sender, text) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut payload = String::new();
        let _ = sender.send(reader.read_to_string(&mut payload).map(|_| payload));
    });
    text.recv_timeout(limit).ok()?.ok()
}

/// Writes a hook state to the agent's terminal as an OSC 7501 report. A provider starts its hooks
/// with no controlling terminal, so the report goes to the nearest ancestor that has one, the
/// agent CLI. A hook must not fail the turn, so a missing terminal only skips the report.
pub(crate) fn report_to_terminal(provider: &str, state: &swarm::host::HookState) {
    use std::io::Write;
    let Some(body) = swarm::host::program_status_report(provider, state) else {
        return;
    };
    let Some(tty) = ancestor_tty(std::process::id()) else {
        return;
    };
    if let Ok(mut terminal) = std::fs::OpenOptions::new().write(true).open(tty) {
        let _ = write!(terminal, "\x1b]7501;{body}\x1b\\");
    }
}

/// The terminal device of `pid` or its nearest ancestor that has one.
pub(crate) fn ancestor_tty(mut pid: u32) -> Option<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    while pid > 1 && seen.insert(pid) {
        let output = std::process::Command::new("ps")
            .args(["-o", "ppid=,tty=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let mut fields = text.split_whitespace();
        let parent = fields.next()?.parse().ok()?;
        match fields.next() {
            // ps prints `??` (macOS) or `?` (Linux) for a process with no terminal.
            Some(tty) if !tty.starts_with('?') => {
                return Some(std::path::Path::new("/dev").join(tty));
            }
            _ => pid = parent,
        }
    }
    None
}

/// `swarm hook`: a hook must never block or fail an agent's turn, so every error goes to stderr
/// and the caller always gets `{}` and exit 0. Providers give a hook about 3 s: stdin gets 2 s,
/// the bus 1 s.
pub(crate) fn hook(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // A caller that is no swarm agent is done before its stdin is read.
    if env::var_os("SWARM_SESSION_ID").is_none() || env::var_os("SWARM_AGENT_ID").is_none() {
        return Ok(());
    }
    let payload = read_within(std::io::stdin(), std::time::Duration::from_secs(2))
        .ok_or("swarm hook: stdin did not close within 2 s")?;
    let Some(report) = hook_report(args, &payload, |name| env::var(name).ok())? else {
        return Ok(());
    };
    // Before the bus, so a busy bus does not lose what the owner sees in the block. Claude Code
    // and Antigravity send their own OSC 7501 reports in Rex, and a second writer flips the
    // record's app, so only Codex, which sends none, gets reports from its hook.
    if let (Some(state), Some(provider)) = (&report.state, args.first())
        && provider == "codex"
        && swarm::host::in_rex(|name| env::var(name).ok())
    {
        report_to_terminal(provider, state);
    }
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    // The providers give a hook about 3 s; a busy bus loses this report rather than the turn.
    connection.busy_timeout(std::time::Duration::from_secs(1))?;
    // A CLI that the agent starts, such as a probe `agy -p`, inherits its SWARM_* variables, and
    // its hook once wrote its own log over a pi chat's log. Only the agent's own CLI speaks for it.
    if agent_cli(&connection, &report.session, &report.agent)?
        .is_some_and(|cli| Some(&cli) != args.first())
    {
        return Ok(());
    }
    if let Some(log) = &report.log {
        store_log(&connection, &report.session, &report.agent, log)?;
    }
    let Some(state) = &report.state else {
        return Ok(());
    };
    if store_state(
        &connection,
        &report.session,
        &report.agent,
        state,
        "hook",
        unix_now()?,
    )? && let Some(provider) = &report.usage_provider
    {
        // Store the state before a usage read can wait on the log or bus.
        let _ = swarm::store::update_usage(&connection, &report.session, &report.agent, provider);
    }
    Ok(())
}

/// The CLI an agent runs: its own row, or the session's chair CLI for the orchestrator. None while
/// neither is known.
pub(crate) fn agent_cli(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row(
            "SELECT COALESCE(a.provider, CASE WHEN ?2 = 'orchestrator' THEN s.chair_provider END)
             FROM session s LEFT JOIN agent a ON a.session_id = s.id AND a.id = ?2
             WHERE s.id = ?1",
            (session, agent),
            |row| row.get(0),
        )
        .optional()?
        .flatten())
}

/// The orchestrator is the chair, so its log is the chat's log. Only `swarm sessions --json`
/// looks a chair log up from its id, and the app never runs it for a live chat.
pub(crate) fn store_log(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    log: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    swarm::store::set_log(connection, session, agent, log)?;
    if agent == "orchestrator" {
        swarm::store::set_chair_log(connection, session, log)?;
    }
    Ok(())
}

/// Stores an agent's own report of its state, from `source` (`hook` or `host`) at `at`, and posts
/// a notice when it starts to wait. False when the session holds no such agent.
pub(crate) fn store_state(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    report: &swarm::host::HookState,
    source: &str,
    at: i64,
) -> Result<bool, Box<dyn std::error::Error>> {
    // An agent the session does not hold has no row to change, so it gets no notice.
    let Some(old) = swarm::store::set_state(
        connection,
        session,
        agent,
        report.state,
        source,
        report.detail.as_deref(),
        report.kind,
        at,
    )?
    else {
        return Ok(false);
    };
    if report.state == "waiting" && old.as_deref() != Some("waiting") {
        notice_waiting(connection, session, agent, report.kind)?;
    }
    Ok(true)
}

/// Reads a pane's raw output until it closes and stores each OSC 7501 report in it, so an agent
/// that speaks the Program Status Protocol needs no swarm hook. The adapter pipes the pane here.
pub(crate) fn status_feed() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let (Ok(session), Ok(agent)) = (env::var("SWARM_SESSION_ID"), env::var("SWARM_AGENT_ID"))
    else {
        return Ok(());
    };
    let mut scanner = swarm::host::StatusScanner::default();
    let mut seen_work = false;
    // pi has no hook to name its log, but `launch` gave it the id `<session>-<agent>` (ADR 0086).
    // pi writes the file just after it reports working and then reports nothing until the turn
    // ends, so after a working report the pane's output looks too, at most once a second.
    let mut logged = false;
    let mut looked: Option<std::time::Instant> = None;
    let mut stdin = std::io::stdin().lock();
    let mut chunk = [0; 8192];
    loop {
        let read = stdin.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        if seen_work
            && !logged
            && looked.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(1))
        {
            looked = Some(std::time::Instant::now());
            if let Some(path) = pi_chair_log(&format!("{session}-{agent}")) {
                match swarm::paths::sqlite_db()
                    .and_then(|db| swarm::store::open(&db))
                    .and_then(|connection| {
                        connection.busy_timeout(std::time::Duration::from_secs(1))?;
                        store_log(&connection, &session, &agent, &path)
                    }) {
                    Ok(()) => logged = true,
                    Err(error) => eprintln!("swarm status-feed: {error}"),
                }
            }
        }
        for body in scanner.push(&chunk[..read]) {
            let Some(report) = swarm::host::program_status(&body, seen_work) else {
                continue;
            };
            seen_work |= report.state == "working";
            let stored = swarm::paths::sqlite_db()
                .and_then(|db| swarm::store::open(&db))
                .and_then(|connection| {
                    connection.busy_timeout(std::time::Duration::from_secs(1))?;
                    if !logged && let Some(path) = pi_chair_log(&format!("{session}-{agent}")) {
                        store_log(&connection, &session, &agent, &path)?;
                        logged = true;
                    }
                    store_state(&connection, &session, &agent, &report, "hook", unix_now()?)
                });
            // The feed must keep reading, or the pane's output backs up; a lost report is the
            // smaller harm, and the next one corrects it.
            if let Err(error) = stored {
                eprintln!("swarm status-feed: {error}");
            }
        }
    }
}

/// ADR 0077: a change to waiting is a notice row, which the notice helper posts.
pub(crate) fn notice_waiting(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    kind: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let folder = swarm::store::session_cwd(connection, session)?
        .map(|cwd| swarm::view::tree::basename(&cwd))
        .unwrap_or_default();
    let what = match kind {
        Some("permission") => "waiting on a permission",
        Some("question") => "asked a question",
        _ => "waiting on a permission or a question",
    };
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
/// its remaining budget.
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
    let _ = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn();
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

#[cfg(test)]
#[path = "hooks/tests.rs"]
mod tests;
