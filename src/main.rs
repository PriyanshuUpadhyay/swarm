use std::env;
use std::os::unix::process::ExitStatusExt;
use swarm::providers::Provider;

#[cfg(feature = "serve")]
mod serve;

const RERING_UNSEEN_AFTER_SECS: i64 = 60;

/// The ring is typed into the recipient's prompt, so it names the next step. An agent that did
/// not load the swarm skill otherwise takes the bare ring as the whole task and waits. The ack comes
/// after the work: an acked message leaves the inbox, so an agent whose context is compacted
/// mid-task can find the task again only while it is unacked. The re-ring keys on `seen_at`, so a
/// late ack never rings twice.
fn ring_text(root: &std::path::Path) -> String {
    format!(
        "swarm: new message. Run swarm inbox and read each body at {}/<body_path>. Run swarm ack <seq> only after you finish that message.",
        root.display()
    )
}

/// How long a ring waits for a starting CLI to draw its input box.
const RING_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// How many more times a ring presses Enter while the input box still holds it.
const RING_ENTER_RETRIES: usize = 3;
/// How long one ring may take in all, its proof wait included, so a hung adapter verb or a lost
/// ring cannot hold the caller for ever.
const RING_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(if cfg!(test) { 5 } else { 30 });
/// How often a ring reads the pane and the store for its proof.
const RING_POLL: std::time::Duration = std::time::Duration::from_millis(500);

/// Now in unix seconds, the unit of `rung_at` and `state_at`.
fn unix_now() -> Result<i64, std::time::SystemTimeError> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64)
}

/// What one ring proved (ADR 0041), stored on `message.delivery`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivery {
    /// The recipient's hook reported a turn after the ring.
    Hook,
    /// The recipient's screen showed a turn or a question after the ring. A busy CLI queues or
    /// steers the ring and fires no turn-start hook (ADR 0038), so this is its only proof.
    Screen,
    /// The recipient read its messages after the ring, though no hook or screen shows a turn now:
    /// a short turn can end between two listing passes, and its `done` hides its turn start.
    Seen,
    /// No proof came before the ring's deadline.
    Unconfirmed,
    /// No proof can come: the pane's CLI is unknown and its screen is no Herdr status.
    Unchecked,
}

impl Delivery {
    fn as_str(self) -> &'static str {
        match self {
            Delivery::Hook => "hook",
            Delivery::Screen => "screen",
            Delivery::Seen => "seen",
            Delivery::Unconfirmed => "unconfirmed",
            Delivery::Unchecked => "unchecked",
        }
    }
}

/// Whether a ring waits for its proof (ADR 0041). The app kills `agents --json` at 20 s, so the
/// listing types its rings and returns, and `settle_rings` in a later pass finds their proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Proof {
    Wait,
    Later,
}

/// Type the ring into `agent`'s `pane` and, with `Proof::Wait`, wait for proof that it started a
/// turn. A CLI that is still starting loses a ring typed before its input box is drawn, or reads
/// the text and its Enter as one paste, so the Enter becomes a newline and the ring sits unsent in
/// the box. So a waiting ring to a CLI whose box swarm can read waits for the box, and presses
/// Enter again while the box still holds the ring. With `Proof::Later` it skips both waits and
/// returns None once the ring is typed; a ring that a starting CLI loses is rung again after 60 s,
/// like any unseen message. A waiting ring also returns None when a store error hides its proof.
fn ring_pane(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    root: &std::path::Path,
    session_id: &str,
    agent_id: &str,
    pane: &str,
    proof: Proof,
) -> Result<Option<Delivery>, Box<dyn std::error::Error>> {
    let text = ring_text(root);
    let provider = swarm::store::provider_of(connection, session_id, agent_id)?;
    let provider = provider.as_deref();
    // The ring's own deadline replaces a batch deadline, so a ring that starts within the batch
    // still gets the time to send its text and Enter, and to see its proof.
    let deadline = std::time::Instant::now() + RING_TIMEOUT;
    let adapter = swarm::adapter::Adapter {
        deadline: Some(deadline),
        ..adapter.clone()
    };
    let vars = [("pane", pane)];
    let read = || {
        adapter
            .capture_within(&vars, std::time::Duration::from_secs(1))
            .unwrap_or_default()
    };
    if proof == Proof::Wait
        && let Some(provider) = provider.filter(|provider| swarm::screen::reads_composer(provider))
    {
        let _ = wait_for_input_box(provider, &read);
    }
    let rung_at = unix_now()?;
    adapter.run("ring", &[("pane", pane), ("text", &text)])?;
    if proof == Proof::Later {
        return Ok(None);
    }
    let mut enters = 0;
    loop {
        std::thread::sleep(RING_POLL);
        let rows = read();
        let screen = screen_of(&adapter, &vars, &rows);
        if provider.is_some_and(|provider| swarm::screen::holds(provider, &rows, &text)) {
            if enters < RING_ENTER_RETRIES && adapter.key.is_some() {
                adapter.run("key", &[("pane", pane), ("key", "Enter")])?;
                enters += 1;
            }
        } else {
            match ring_proof(
                connection,
                session_id,
                agent_id,
                provider,
                &rows,
                screen.as_deref(),
                rung_at,
            ) {
                Ok(None) => {}
                Ok(delivery) => return Ok(delivery),
                // A store error proves nothing either way, so the ring keeps no result and a
                // later sweep or listing pass settles it.
                Err(error) => {
                    eprintln!("swarm: ring proof not read: {error}");
                    return Ok(None);
                }
            }
        }
        if std::time::Instant::now() + RING_POLL >= deadline {
            return Ok(Some(Delivery::Unconfirmed));
        }
    }
}

/// Wait until `provider`'s CLI draws its input box, for at most `RING_READY_TIMEOUT`. A question on
/// screen, such as a folder trust prompt, also ends the wait. True when the first read ends it, so
/// the CLI was not starting.
fn wait_for_input_box(provider: &str, read: &impl Fn() -> String) -> bool {
    let ready = std::time::Instant::now() + RING_READY_TIMEOUT;
    let mut first = true;
    while std::time::Instant::now() < ready {
        let rows = read();
        if swarm::screen::composer(provider, &rows).is_some()
            || swarm::screen::prompt(&rows).is_some()
        {
            return first;
        }
        first = false;
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

/// Type the owner's `text` into `agent_id`'s `pane`, for `swarm type`. A CLI that is still
/// starting loses text typed before its input box is drawn, or keeps it unsent in the box, so as a
/// ring does, the text waits for a box that swarm can read, and Enter is pressed again while the
/// box still holds it. A CLI whose box was already drawn takes the text and its Enter, so the
/// owner's send to it returns at once.
fn type_text(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent_id: &str,
    pane: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let provider = swarm::store::provider_of(connection, session_id, agent_id)?
        .filter(|provider| swarm::screen::reads_composer(provider));
    let vars = [("pane", pane)];
    let read = || {
        adapter
            .capture_within(&vars, std::time::Duration::from_secs(1))
            .unwrap_or_default()
    };
    let started = provider
        .as_deref()
        .is_some_and(|provider| wait_for_input_box(provider, &read));
    adapter.run("ring", &[("pane", pane), ("text", text)])?;
    let Some(provider) = provider.filter(|_| !started && adapter.key.is_some()) else {
        return Ok(());
    };
    for _ in 0..RING_ENTER_RETRIES {
        std::thread::sleep(RING_POLL);
        if !swarm::screen::holds(&provider, &read(), text) {
            break;
        }
        adapter.run("key", &[("pane", pane), ("key", "Enter")])?;
    }
    Ok(())
}

/// The pane's screen for `ring_proof`: the `screen` verb's output, or the capture `rows` when the
/// adapter has no `screen` verb. None when the `screen` verb failed.
fn screen_of(
    adapter: &swarm::adapter::Adapter,
    vars: &[(&str, &str)],
    rows: &str,
) -> Option<String> {
    match adapter.screen {
        Some(_) => adapter.screen(vars, std::time::Duration::from_secs(1)),
        None => Some(rows.to_string()),
    }
}

/// What the store and the pane prove now about a ring typed at `rung_at`, if anything. The
/// caller checks first that the input box no longer holds the ring. A `screen` of None, a failed
/// read, proves nothing, so the pane is not taken as one with no reader. A store error is
/// returned, not read as no proof, so a late ring is not stored as unconfirmed because of it.
fn ring_proof(
    connection: &rusqlite::Connection,
    session_id: &str,
    agent_id: &str,
    provider: Option<&str>,
    rows: &str,
    screen: Option<&str>,
    rung_at: i64,
) -> Result<Option<Delivery>, Box<dyn std::error::Error>> {
    if provider.is_none()
        && screen.is_some_and(|screen| swarm::screen::herdr_state(screen).is_none())
    {
        return Ok(Some(Delivery::Unchecked));
    }
    if swarm::store::turn_started(connection, session_id, agent_id, rung_at)? {
        return Ok(Some(Delivery::Hook));
    }
    if let Some((swarm::screen::ScreenState::Working | swarm::screen::ScreenState::Waiting, _)) =
        swarm::screen::read_pane(provider, screen.unwrap_or(rows), || Some(rows.to_string()))
    {
        return Ok(Some(Delivery::Screen));
    }
    Ok(
        swarm::store::seen_since(connection, session_id, agent_id, rung_at)?
            .then_some(Delivery::Seen),
    )
}

/// Ring `agent` for the messages `seqs`, stored as rung at `rung_at`, and store what the ring
/// proved on them. The bell is a hint (R9), so a failure only warns, and a failed ring is
/// unconfirmed. With `Proof::Later` a ring, typed or failed, stores nothing and returns None, and
/// so does a ring whose proof read failed in the store; `settle_rings` stores its proof, so the
/// chair's own failed ring is still left to the sweep line. A result that this call did not store,
/// because the write failed or another pass stored first, is None too, so only the pass that
/// stores a ring's result reports it.
#[allow(clippy::too_many_arguments)]
fn ring_and_record(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    adapter: Result<swarm::adapter::Adapter, Box<dyn std::error::Error>>,
    session_id: &str,
    agent: &str,
    pane: &str,
    seqs: &[i64],
    rung_at: i64,
    proof: Proof,
) -> Option<Delivery> {
    let delivery = adapter
        .and_then(|adapter| ring_pane(connection, &adapter, root, session_id, agent, pane, proof))
        .unwrap_or_else(|error| {
            eprintln!("swarm: ring failed: {error}");
            (proof == Proof::Wait).then_some(Delivery::Unconfirmed)
        })?;
    if delivery == Delivery::Unconfirmed {
        eprintln!(
            "swarm: {agent} started no turn within {} s; ring unconfirmed",
            RING_TIMEOUT.as_secs()
        );
    }
    match swarm::store::set_delivery(connection, session_id, seqs, rung_at, delivery.as_str()) {
        Ok(stored) => stored.then_some(delivery),
        Err(error) => {
            eprintln!("swarm: ring result not stored: {error}");
            None
        }
    }
}

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    let adapters = swarm::paths::root_dir()?.join("adapters");
    std::fs::create_dir_all(&adapters)?;
    // Nothing shipped is written here any more. The binary carries the adapters
    // (`swarm::adapter::SHIPPED`) and a file on disk states an edit, so a copy that says what the
    // binary already says is a copy that can only go stale. See `swarm::adapter::load`.
    for (name, text) in swarm::adapter::SHIPPED {
        let file = adapters.join(format!("{name}.conf"));
        if std::fs::read_to_string(&file).is_ok_and(|deployed| deployed == text) {
            std::fs::remove_file(&file)?;
            println!("swarm: removed adapters/{name}.conf, which said what this binary says");
        }
    }
    swarm::store::open(&swarm::paths::sqlite_db()?)?;
    // Setting up the hooks changes the owner's Codex and AGY config, so it waits for consent.
    println!(
        "swarm: to see Codex and AGY agents' chats and questions, run `swarm setup hooks`; it trusts swarm's own Codex hooks and adds swarm's AGY hooks"
    );

    Ok(())
}

const USAGE: &str = "usage: swarm --version | init | doctor [--json] | setup [--json] | setup status --json | setup <all|hooks,trust,plugin,hud> [--plan [--json] | --digest <digest> | --yes] [--cwd <dir>] [--consent <standing|ask>] [--resume] | hooks status --json | managed list [--json] | managed revert (<id>... | --all) [--plan [--json] | --digest <digest>] | adapter check <name> | session new <talk_mode> [--chair <claude|codex|pi>:<id>] (cwd: pwd -P) | session chair <claude|codex|pi>:<id> | session continue <new_id> <old_id> | session archive <id>... | session unarchive <id>... | sessions --json [--archived] | agent add <agent_id> <role> | serve [--port N] [--assets DIR] | ui [stop | --foreground] [--port N] [--assets DIR] | herdr-split | notify <title> [--body <text>] | hud <uninstall|status> [--json] | notices --json | notices --due | host-context --provider <claude|codex|agy> | hook <claude|codex|agy> [event] | status-feed | guard <claude|codex|agy> PreToolUse | roles --json | roles get <role> [--provider <claude|codex|agy|pi>] | roles check --json | roles save --revision <revision> <profile-json> | roles new <name> --revision <revision> | roles rename <old> <new> --revision <revision> | roles copy <from> <to> --revision <revision> | roles delete <name> --revision <revision> | roles reset --revision <revision> | roles set-min-usage <pct> --revision <revision> | providers --json | models --provider <claude|codex|agy|pi> --json | accounts --provider <claude|codex|agy|pi> --json | accounts login --provider <claude|codex> --name <name> --revision <revision> --json [--cwd <dir>] | accounts reset --revision <revision> --json | accounts pick --provider <claude|codex> [--account <auto|menu|NAME> | --session <id> | --last [--all]] [--model <model>] --json | usage --json | usage --format openusage | working --json | usage --refresh --provider <claude|codex> --json | agents --json [--all] | messages --json [--after <seq>] | run <claude|codex> [--account [auto|menu|NAME]] [-- <args>...] | launch <agent_id> <role> [--provider <claude|codex|agy|pi>] [--model <model> for chat] [--account <auto|name>] [--cwd <dir>] [-- <provider args>...] | spawn <agent_id> <role> [--provider <p>] [--account <auto|name>] [-- <cmd>...] | type <agent_id> | answer <agent_id> <prompt_id> <choice> | interrupt <agent_id> | key <agent_id> <Up|C-u> | attach <agent_id> | focus <session_id> <agent_id> | close <agent_id> | send <recipient> <kind> | commands <agent_id> --json | finish | exited | sweep [--every <secs>] | drain | inbox | ack <seq>";

fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("swarm: {name} not set"))
}

fn session_id() -> Result<String, String> {
    match env::var("SWARM_SESSION_ID") {
        Ok(session) => valid_session_id(&session),
        Err(_) => Ok(identity()?.0),
    }
}

fn valid_session_id(id: &str) -> Result<String, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "swarm: bad SWARM_SESSION_ID".to_string())?;
    if id.get_version_num() != 7 {
        return Err("swarm: bad SWARM_SESSION_ID".to_string());
    }
    Ok(id.to_string())
}

/// Every agent pane has one of these set. It is a guard against a model's mistake, not a trust
/// boundary.
fn in_agent_pane() -> bool {
    ["TMUX_PANE", "HERDR_PANE_ID", "SUPERSET_TERMINAL_ID"]
        .iter()
        .any(|name| env::var_os(name).is_some())
}

/// What one hook call reports: the session and agent, the state with its detail and wait kind,
/// and the chat log path from the payload.
#[derive(Debug, PartialEq)]
struct HookReport {
    session: String,
    agent: String,
    state: Option<swarm::host::HookState>,
    log: Option<std::path::PathBuf>,
    usage_provider: Option<String>,
}

/// The chat log a hook payload names, only when it is an absolute path to an existing `.jsonl`
/// file. Claude and Codex send `transcript_path`, which Codex may leave null; AGY sends
/// `transcriptPath`.
fn hook_log(payload: &serde_json::Value) -> Option<std::path::PathBuf> {
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
fn hook_report(
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
fn read_within<R: std::io::Read + Send + 'static>(
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

/// `swarm hook`: a hook must never block or fail an agent's turn, so every error goes to stderr
/// and the caller always gets `{}` and exit 0. Providers give a hook about 3 s: stdin gets 2 s,
/// the bus 1 s.
fn hook(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // A caller that is no swarm agent is done before its stdin is read.
    if env::var_os("SWARM_SESSION_ID").is_none() || env::var_os("SWARM_AGENT_ID").is_none() {
        return Ok(());
    }
    let payload = read_within(std::io::stdin(), std::time::Duration::from_secs(2))
        .ok_or("swarm hook: stdin did not close within 2 s")?;
    let Some(report) = hook_report(args, &payload, |name| env::var(name).ok())? else {
        return Ok(());
    };
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    // The providers give a hook about 3 s; a busy bus loses this report rather than the turn.
    connection.busy_timeout(std::time::Duration::from_secs(1))?;
    if let Some(log) = &report.log {
        store_log(&connection, &report.session, &report.agent, log)?;
    }
    let Some(state) = &report.state else {
        return Ok(());
    };
    if store_state(&connection, &report.session, &report.agent, state)?
        && let Some(provider) = &report.usage_provider
    {
        // Store the state before a usage read can wait on the log or bus.
        let _ = swarm::store::update_usage(&connection, &report.session, &report.agent, provider);
    }
    Ok(())
}

/// The orchestrator is the chair, so its log is the chat's log. Only `swarm sessions --json`
/// looks a chair log up from its id, and the app never runs it for a live chat.
fn store_log(
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

/// Stores an agent's own report of its state, and posts a notice when it starts to wait. False
/// when the session holds no such agent.
fn store_state(
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
    report: &swarm::host::HookState,
) -> Result<bool, Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    // An agent the session does not hold has no row to change, so it gets no notice.
    let Some(old) = swarm::store::set_state(
        connection,
        session,
        agent,
        report.state,
        "hook",
        report.detail.as_deref(),
        report.kind,
        now,
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
fn status_feed() -> Result<(), Box<dyn std::error::Error>> {
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
                    store_state(&connection, &session, &agent, &report)
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
fn notice_waiting(
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
fn start_notify_helper() {
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
fn notify_helper() -> Option<std::path::PathBuf> {
    if let Some(path) = env::var_os("SWARM_NOTIFY_APP") {
        return Some(std::path::PathBuf::from(path)).filter(|path| path.exists());
    }
    swarm::paths::bundled("SwarmNotify.app")
}

/// ADR 0077: take every unposted notice and print the ones the owner's notice preferences post,
/// each with its sound choice. A row whose agent is not waiting is a done notice.
fn notices_due() -> Result<(), Box<dyn std::error::Error>> {
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
        if waiting || on("done") {
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

fn adapter_name() -> String {
    swarm::host::adapter(|name| env::var(name).ok())
}

/// The host contract that `swarm host-context` would print in a pane that `launch` is about to
/// open, for a CLI with no SessionStart hook. The pane gets these ids, and a tmux-swarm pane
/// always has `TMUX_PANE`.
fn pane_context(provider: &str, session: &str, agent_id: &str) -> Option<String> {
    let adapter = adapter_name();
    swarm::host::context(provider, |name| match name {
        "SWARM_SESSION_ID" => Some(session.into()),
        "SWARM_AGENT_ID" => Some(agent_id.into()),
        "SWARM_ADAPTER" => Some(adapter.clone()),
        "TMUX_PANE" if adapter == "tmux-swarm" => Some("pane".into()),
        _ => env::var(name).ok(),
    })
}

/// A pane that `swarm launch` made carries its ids in its env. A chair keeps none: `swarm session
/// new` records the chair's pane, and the chair is found from that pane.
fn identity() -> Result<(String, String), String> {
    if let Ok(session) = env::var("SWARM_SESSION_ID") {
        return Ok((valid_session_id(&session)?, env_var("SWARM_AGENT_ID")?));
    }
    let found = own_pane().and_then(|pane| {
        let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
        swarm::store::orchestrator_at_pane(&connection, &pane)
    });
    match found {
        Ok(Some(identity)) => Ok(identity),
        _ => Err("swarm: this pane has no swarm session; run `swarm session new lane`".into()),
    }
}

/// The pane this process runs in, as the adapter's `self` verb reports it.
fn own_pane() -> Result<String, Box<dyn std::error::Error>> {
    let pane =
        swarm::adapter::load(&swarm::paths::root_dir()?, &adapter_name())?.run("self", &[])?;
    match pane.trim().is_empty() {
        true => Err("swarm: the adapter gave no pane".into()),
        false => Ok(pane),
    }
}

fn run_tool(
    executable: &str,
    args: &[&str],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    std::process::Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| format!("swarm: cannot run {executable}: {error}").into())
}

fn tool_stdout(executable: &str, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    checked_stdout(executable, run_tool(executable, args)?)
}

fn checked_stdout(
    executable: &str,
    output: std::process::Output,
) -> Result<String, Box<dyn std::error::Error>> {
    if !output.status.success() {
        return Err(format!(
            "swarm: {executable} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("swarm: {executable} printed non-UTF-8 output: {error}").into())
}

/// Whether each runner can start now (ADR 0032). Each provider's accounts are read at most once.
struct Probe {
    deadline: std::time::Instant,
    min_usage_left_pct: u8,
    /// `--account <name>`: only that account counts. None or `auto` counts every account.
    account: Option<String>,
    accounts: std::cell::RefCell<
        std::collections::HashMap<Provider, Result<swarm::profiles::AccountList, String>>,
    >,
}

impl Probe {
    fn new(config: &swarm::config::Config, account: Option<&str>) -> Probe {
        // Account selection starts its budget after launch has checked the host and opened the store.
        Probe {
            deadline: swarm::profiles::native::deadline(ACCOUNT_PICK_TIMEOUT_SECONDS),
            min_usage_left_pct: config.min_usage_left_pct,
            account: account.filter(|name| *name != "auto").map(str::to_string),
            accounts: Default::default(),
        }
    }

    fn check(&self, runner: &swarm::config::Runner) -> Option<(swarm::config::SkipCode, String)> {
        let provider = runner.provider;
        if !installed(provider.id()) {
            return Some((
                swarm::config::SkipCode::CliMissing,
                format!("{} CLI not found on PATH", provider.id()),
            ));
        }
        if !provider.has_accounts() {
            return None;
        }
        let mut cache = self.accounts.borrow_mut();
        let accounts = cache
            .entry(provider)
            .or_insert_with(|| pick_accounts(provider, self.deadline, self.account.is_none()));
        // An explicit account bypasses the Fable guard; authentication and general limits still apply.
        let model = self.account.is_none().then_some(runner.model.as_str());
        let scoped = accounts.as_ref().ok().map(|list| list.for_model(model));
        if let Some(list) = &scoped
            && let Err(reason) = swarm::profiles::pick_auto(
                &list.accounts,
                list.current_home.as_deref(),
                model,
                self.min_usage_left_pct,
            )
        {
            return Some((swarm::config::SkipCode::FableSpent, reason));
        }
        let accounts = scoped
            .as_ref()
            .and_then(|list| account_states(list, self.account.as_deref(), model));
        // A named account belongs to one provider; another provider's runner cannot use it.
        if let (Some(name), Some([])) = (&self.account, accounts.as_deref()) {
            return Some((
                swarm::config::SkipCode::SignedOut,
                format!("no {} account named {name}", provider.id()),
            ));
        }
        swarm::config::account_skip(provider, accounts.as_deref(), self.min_usage_left_pct)
    }
}

/// Native authentication uncertainty remains a read failure for ADR 0032.
fn account_states(
    list: &swarm::profiles::AccountList,
    only: Option<&str>,
    model: Option<&str>,
) -> Option<Vec<swarm::config::AccountState>> {
    let rows: Vec<_> = list
        .accounts
        .iter()
        .filter(|account| !account.invalid_home())
        .filter(|account| only.is_none_or(|name| account.name == name))
        .filter(|account| !swarm::profiles::is_fable_model(model) || !account.fable_spent)
        .collect();
    if rows
        .iter()
        .any(|account| account.auth_state == swarm::profiles::AuthState::Unavailable)
    {
        return None;
    }
    Some(
        rows.into_iter()
            .map(|account| swarm::config::AccountState {
                signed_in: account.auth_state == swarm::profiles::AuthState::SignedIn,
                remaining_pct: (account.usage_state == "fresh")
                    .then_some(account.remaining_pct)
                    .flatten(),
            })
            .collect(),
    )
}

/// The runner `role` starts with, as `roles get` prints it. Each skipped runner is one stderr
/// line, and `running` adds the line that names the runner that starts.
fn resolve_role(
    role: &str,
    provider: Option<&str>,
    account: Option<&str>,
    running: bool,
) -> Result<(serde_json::Value, Probe), String> {
    let (config, _) = swarm::config::load().map_err(|error| format!("swarm: {error}"))?;
    let profile = config
        .profile(role)
        .map_err(|error| format!("swarm: {error}"))?;
    let only = provider
        .map(|name| Provider::parse(name).ok_or_else(|| format!("swarm: unknown provider {name}")))
        .transpose()?;
    if let Some(only) = only
        && !profile.runners.iter().any(|runner| runner.provider == only)
    {
        return Err(format!(
            "swarm: {role}: profile has no {} runner",
            only.id()
        ));
    }
    let probe = Probe::new(&config, account);
    let selection = swarm::config::select(profile, only, |runner| probe.check(runner));
    let id = |index: usize| format!("{role}#{}", index + 1);
    let Some(index) = selection.pick else {
        let lines: Vec<String> = selection
            .skipped
            .iter()
            .map(|skip| {
                format!(
                    "  {} {}: {}",
                    skip.index + 1,
                    profile.runners[skip.index].label(),
                    skip.text
                )
            })
            .collect();
        return Err(format!(
            "swarm: {role}: no runner can run\n{}",
            lines.join("\n")
        ));
    };
    // The error above lists each skip once; these lines only go with a runner that starts.
    for skip in &selection.skipped {
        eprintln!(
            "swarm: {role}: skipped {}: {}",
            profile.runners[skip.index].label(),
            skip.text
        );
    }
    let runner = &profile.runners[index];
    if running {
        eprintln!("swarm: {role}: running {}", runner.label());
    }
    let mut value = serde_json::to_value(runner).map_err(|error| error.to_string())?;
    let fields = value
        .as_object_mut()
        .expect("a runner serializes to an object");
    fields.insert("role".into(), role.into());
    fields.insert("runnerId".into(), id(index).into());
    let fallbacks: Vec<String> = (0..profile.runners.len())
        .filter(|other| *other != index)
        .map(id)
        .collect();
    fields.insert("fallbackRunnerIds".into(), fallbacks.into());
    if let Some(first) = selection.skipped.first() {
        fields.insert("substitutedFor".into(), id(first.index).into());
    }
    fields.insert(
        "skipped".into(),
        serde_json::to_value(&selection.skipped).map_err(|error| error.to_string())?,
    );
    Ok((value, probe))
}

/// A provider counts as installed when an executable file of its name is on PATH, because that
/// binary is what a pane runs.
fn installed(provider: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|dir| {
            std::fs::metadata(dir.join(provider))
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
    })
}

fn load_accounts(
    provider: &str,
    deadline: std::time::Instant,
) -> Result<swarm::profiles::AccountList, Box<dyn std::error::Error>> {
    let provider =
        Provider::parse(provider).ok_or_else(|| format!("swarm: unknown provider {provider}"))?;
    swarm::profiles::native::load(provider, deadline)
        .map_err(|error| format!("swarm: {error}").into())
}

/// The accounts for a pick. An automatic pick ranks accounts by usage left, so it first refreshes
/// a usage cache that is missing or older than the freshness limit; a named pick reads no usage.
fn pick_accounts(
    provider: Provider,
    deadline: std::time::Instant,
    auto: bool,
) -> Result<swarm::profiles::AccountList, String> {
    let mut accounts = swarm::profiles::native::load(provider, deadline)?;
    if auto
        && let Some(meters) = swarm::usage::refresh_if_old(
            provider,
            &accounts,
            swarm::profiles::native::deadline(USAGE_PICK_REFRESH_TIMEOUT_SECONDS),
        )
    {
        swarm::profiles::apply_usage(&mut accounts, &meters);
    }
    Ok(accounts)
}

/// `--account menu`: the accounts as a numbered list on `out`, then the choice read from `input`.
/// Enter alone is `auto`; a number or a name picks that account.
fn menu_choice(
    accounts: &swarm::profiles::AccountList,
    input: &mut impl std::io::BufRead,
    out: &mut impl std::io::Write,
) -> Result<String, String> {
    let names: Vec<&str> = accounts.accounts.iter().map(|a| a.name.as_str()).collect();
    if names.is_empty() {
        return Err(format!(
            "swarm: no {} accounts; add one with swarm accounts login --provider {} --name NAME",
            accounts.provider, accounts.provider
        ));
    }
    let width = names.iter().map(|n| n.len()).max().unwrap_or(0);
    let write = |out: &mut dyn std::io::Write, text: String| {
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())
    };
    for (index, account) in accounts.accounts.iter().enumerate() {
        let left = match (&account.auth_state, account.remaining_pct) {
            (swarm::profiles::AuthState::SignedIn, Some(pct)) => format!("{pct}% left"),
            (swarm::profiles::AuthState::SignedIn, None) => "usage unknown".into(),
            _ => "signed out".into(),
        };
        write(
            out,
            format!(
                "{:>2}) {:width$}  {:28}  {left}\n",
                index + 1,
                account.name,
                account.email.as_deref().unwrap_or("")
            ),
        )?;
    }
    write(out, format!("Account [1-{}, Enter = auto]: ", names.len()))?;
    out.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|e| e.to_string())?;
    let line = line.trim();
    if line.is_empty() {
        return Ok("auto".into());
    }
    match line.parse::<usize>() {
        Ok(n) if (1..=names.len()).contains(&n) => Ok(names[n - 1].into()),
        _ if names.contains(&line) => Ok(line.into()),
        _ => Err(format!("swarm: no account {line:?} in the list")),
    }
}

/// Shows the menu on the terminal, not on stdout, so `accounts pick --json` output stays JSON.
fn ask_account(
    accounts: Result<&swarm::profiles::AccountList, String>,
    model: Option<&str>,
) -> Result<String, String> {
    let accounts = accounts
        .map_err(|error| format!("swarm: {error}"))?
        .for_model(model);
    let tty = std::fs::File::open("/dev/tty")
        .map_err(|_| "swarm: --account menu needs a terminal; name the account instead")?;
    menu_choice(
        &accounts,
        &mut std::io::BufReader::new(tty),
        &mut std::io::stderr(),
    )
}

fn select_account(
    provider: Provider,
    requested: &str,
    model: Option<&str>,
    min_usage_left_pct: u8,
    accounts: Result<&swarm::profiles::AccountList, String>,
) -> Result<Option<swarm::profiles::Account>, String> {
    let mut accounts = accounts.map(|accounts| accounts.for_model(model));
    if requested == "auto"
        && let Ok(accounts) = &mut accounts
    {
        accounts.auto = swarm::profiles::pick_auto(
            &accounts.accounts,
            accounts.current_home.as_deref(),
            model,
            min_usage_left_pct,
        )
        .map_err(|error| format!("swarm: {error}"))?;
    }
    match accounts
        .and_then(|accounts| swarm::profiles::resolve_account(&accounts, requested).cloned())
    {
        Ok(account) => Ok(Some(account)),
        Err(error) if requested == "auto" => {
            eprintln!("swarm: {error}; {} uses its own login", provider.id());
            Ok(None)
        }
        Err(error) => Err(format!("swarm: {error}")),
    }
}

fn run_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::process::CommandExt;
    let (options, extra) = match args.iter().position(|arg| arg == "--") {
        Some(index) => (&args[..index], &args[index + 1..]),
        None => (args, &[][..]),
    };
    let (provider, requested) = match options {
        [provider] => (provider, "auto"),
        // `--account` with no name opens the menu.
        [provider, flag] if flag == "--account" => (provider, "menu"),
        [provider, flag, account] if flag == "--account" => (provider, account.as_str()),
        _ => return Err(USAGE.into()),
    };
    let provider = Provider::parse(provider)
        .filter(|provider| provider.has_accounts())
        .ok_or("swarm: run requires claude or codex")?;
    let accounts = pick_accounts(
        provider,
        swarm::profiles::native::deadline(ACCOUNT_PICK_TIMEOUT_SECONDS),
        matches!(requested, "auto" | "menu"),
    );
    let chosen;
    let requested = if requested == "menu" {
        chosen = ask_account(
            accounts.as_ref().map_err(Clone::clone),
            swarm::bus::command_model(extra),
        )?;
        chosen.as_str()
    } else {
        requested
    };
    let account = select_account(
        provider,
        requested,
        swarm::bus::command_model(extra),
        swarm::config::min_usage_left_pct(),
        accounts.as_ref().map_err(Clone::clone),
    )?;
    let mut command = std::process::Command::new(provider.id());
    command.args(extra);
    for key in provider.account_env_keys() {
        command.env_remove(key);
    }
    if let Some(account) = account {
        eprintln!("account {}", account.name);
        command.envs(account.env);
    }
    Err(format!("swarm: cannot run {}: {}", provider.id(), command.exec()).into())
}

/// The account `swarm run` would use, for a shell wrapper such as `pi` that starts the CLI itself.
/// `clear` names the account variables to unset first; `name` is null when the CLI keeps its own
/// login. `--session ID` or `--last [--all]` picks the Codex account that holds that session.
fn account_pick(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = std::collections::BTreeMap::new();
    let mut flags = std::collections::BTreeSet::new();
    let mut arguments = args.iter();
    while let Some(flag) = arguments.next() {
        if ["--json", "--last", "--all"].contains(&flag.as_str()) {
            if !flags.insert(flag.as_str()) {
                return Err(USAGE.into());
            }
            continue;
        }
        if !["--provider", "--account", "--model", "--session"].contains(&flag.as_str()) {
            return Err(USAGE.into());
        }
        let value = arguments.next().ok_or(USAGE)?;
        if options.insert(flag.as_str(), value.as_str()).is_some() {
            return Err(USAGE.into());
        }
    }
    let provider = options
        .get("--provider")
        .and_then(|name| Provider::parse(name))
        .filter(|provider| provider.has_accounts())
        .ok_or("swarm: accounts pick needs claude or codex")?;
    use swarm::profiles::native::CodexSession;
    let session = match (options.get("--session"), flags.contains("--last")) {
        (Some(id), false) if !flags.contains("--all") => Some(CodexSession::Id(id.to_string())),
        (None, true) => Some(CodexSession::Last(
            (!flags.contains("--all"))
                .then(std::env::current_dir)
                .transpose()?,
        )),
        (None, false) if !flags.contains("--all") => None,
        _ => return Err(USAGE.into()),
    };
    if !flags.contains("--json") || session.is_some() && options.contains_key("--account") {
        return Err(USAGE.into());
    }
    if session.is_some() && provider != Provider::Codex {
        return Err("swarm: --session and --last pick only codex accounts".into());
    }
    let mut requested = options
        .get("--account")
        .copied()
        .unwrap_or("auto")
        .to_string();
    let accounts = pick_accounts(
        provider,
        swarm::profiles::native::deadline(ACCOUNT_PICK_TIMEOUT_SECONDS),
        matches!(requested.as_str(), "auto" | "menu") && session.is_none(),
    );
    if requested == "menu" {
        requested = ask_account(
            accounts.as_ref().map_err(Clone::clone),
            options.get("--model").copied(),
        )?;
    }
    if let Some(session) = &session {
        let list = accounts
            .as_ref()
            .map_err(|error| format!("swarm: {error}"))?;
        requested = swarm::profiles::native::codex_session_owner(&list.accounts, session)
            .ok_or(match session {
                CodexSession::Id(id) => format!("swarm: session {id} is not in any codex account"),
                CodexSession::Last(Some(cwd)) => {
                    format!("swarm: no codex session in {}", cwd.display())
                }
                CodexSession::Last(None) => "swarm: no codex session in any account".into(),
            })?
            .name
            .clone();
    }
    let account = select_account(
        provider,
        &requested,
        options.get("--model").copied(),
        swarm::config::min_usage_left_pct(),
        accounts.as_ref().map_err(Clone::clone),
    )?;
    print_json(&serde_json::json!({
        "schema": "swarm.account-pick.v1",
        "name": account.as_ref().map(|a| &a.name),
        "home": account.as_ref().map(|a| &a.home),
        "env": account.as_ref().map(|a| a.env.clone()).unwrap_or_default(),
        "clear": provider.account_env_keys(),
    }))
}

fn account_login(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if swarm::host::is_worker(|name| env::var(name).ok()) {
        return Err("swarm: only the owner or orchestrator can open a login pane".into());
    }
    let mut options = std::collections::BTreeMap::new();
    let mut json = false;
    let mut arguments = args.iter();
    while let Some(flag) = arguments.next() {
        if flag == "--json" && !json {
            json = true;
            continue;
        }
        if !["--provider", "--name", "--revision", "--cwd"].contains(&flag.as_str()) {
            return Err("swarm: invalid accounts login option".into());
        }
        let value = arguments
            .next()
            .ok_or("swarm: missing accounts login value")?;
        if options.insert(flag.as_str(), value.as_str()).is_some() {
            return Err("swarm: duplicate accounts login option".into());
        }
    }
    if !json {
        return Err("swarm: accounts login requires --json".into());
    }
    let provider = options
        .get("--provider")
        .and_then(|name| Provider::parse(name))
        .filter(|provider| provider.has_accounts())
        .ok_or("swarm: login needs claude or codex")?;
    let name = options.get("--name").ok_or("swarm: login needs --name")?;
    let expected = options
        .get("--revision")
        .ok_or("swarm: login needs --revision")?;
    let cwd = options
        .get("--cwd")
        .map_or_else(env::current_dir, |value| {
            Ok(std::path::PathBuf::from(value))
        })?;
    if !cwd.is_absolute() || !cwd.is_dir() {
        return Err("swarm: --cwd must be an existing absolute directory".into());
    }
    let cwd = std::fs::canonicalize(cwd).map_err(|_| "swarm: --cwd is unavailable")?;
    let deadline = swarm::profiles::native::deadline(LOGIN_ADAPTER_TIMEOUT_SECONDS);
    let (entry, revision) = swarm::accounts::register(provider, name, expected)?;
    let root = swarm::paths::root_dir()?;
    let adapter = swarm::adapter::Adapter {
        deadline: Some(deadline),
        ..swarm::adapter::load(&root, &adapter_name())
            .map_err(|_| "swarm: account registered; login pane adapter is unavailable")?
    };
    env::set_current_dir(cwd)?;
    // Eight label characters and a UUID suffix keep the unique attempt ID below 40 bytes.
    let label: String = name
        .chars()
        .take(8)
        .map(|ch| if matches!(ch, '.' | '_') { '-' } else { ch })
        .collect();
    let attempt = uuid::Uuid::now_v7().simple().to_string();
    let pane_name = format!("login-{}-{label}-{}", provider.id(), &attempt[16..]);
    let session = env::var("SWARM_SESSION_ID").unwrap_or_default();
    let home = swarm::paths::home()?;
    let pane = adapter
        .run(
            "spawn",
            &[
                ("agent_id", &pane_name),
                ("session_id", &session),
                ("home", &home),
                ("adapter", &adapter.name),
            ],
        )
        .map_err(|_| "swarm: account registered; cannot open login pane")?;
    if pane.is_empty() || pane.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("swarm: account registered; login pane has no valid ID".into());
    }
    let native_env = provider.identity_env(name, &entry.home.to_string_lossy());
    let mut command = vec!["env".to_string()];
    for key in provider.account_env_keys() {
        command.extend(["-u".into(), key.to_string()]);
    }
    command.push("--".into());
    command.extend(
        native_env
            .iter()
            .map(|(key, value)| format!("{key}={value}")),
    );
    command.extend(
        provider
            .login_argv()
            .ok_or("swarm: native login is unavailable")?
            .iter()
            .map(|arg| arg.to_string()),
    );
    // Keep a failed login and its error text visible; a successful login closes the shell.
    let text = format!("{} && exit", swarm::adapter::shell_line(&command));
    if adapter
        .run("ring", &[("pane", &pane), ("text", &text)])
        .is_err()
    {
        // Cleanup also runs after ring exhausts its budget, within the Swift process limit.
        let closer = swarm::adapter::Adapter {
            deadline: Some(swarm::profiles::native::deadline(
                LOGIN_PANE_CLOSE_TIMEOUT_SECONDS,
            )),
            ..adapter
        };
        if closer.run("close", &[("pane", &pane)]).is_err() {
            return Err(format!(
                "swarm: account registered; cannot start native login; pane {pane} stayed open because close failed"
            )
            .into());
        }
        return Err("swarm: account registered; cannot start native login in pane".into());
    }
    print_json(
        &serde_json::json!({"provider":provider.id(), "account":entry.name, "pane":pane, "state":"opened", "revision":revision}),
    )
}

fn print_json(value: &impl serde::Serialize) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

/// A printed format with a version (`docs/usage-format.md`): `schema` comes first, then the body.
#[derive(serde::Serialize)]
struct Versioned<'a, T> {
    schema: &'static str,
    #[serde(flatten)]
    body: &'a T,
}

fn valid_chair_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn parse_chair(value: &str) -> Result<Option<(&str, &str)>, String> {
    let (provider, id) = value.split_once(':').ok_or_else(|| USAGE.to_string())?;
    if !matches!(provider, "claude" | "codex" | "pi") {
        return Err(USAGE.to_string());
    }
    Ok(valid_chair_id(id).then_some((provider, id)))
}

fn env_chair() -> Option<(String, String)> {
    for (provider, variable) in [
        ("claude", "CLAUDE_CODE_SESSION_ID"),
        ("codex", "CODEX_THREAD_ID"),
        ("pi", "PI_SESSION_ID"),
    ] {
        if let Ok(id) = env::var(variable) {
            return valid_chair_id(&id).then(|| (provider.to_string(), id));
        }
    }
    None
}

fn default_codex_home_with_env(
    mut env_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> Result<std::path::PathBuf, String> {
    if let Some(home) = env_var("CODEX_HOME") {
        return Ok(home.into());
    }
    env_var("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".codex"))
        .ok_or_else(|| "swarm: HOME not set".to_string())
}

fn default_codex_home() -> Result<std::path::PathBuf, String> {
    default_codex_home_with_env(|variable| std::env::var_os(variable))
}

/// The native Codex home, each discovered `~/.codex-<name>` home, and each home that
/// `swarm accounts login` made under `accounts/codex/` in swarm's folder.
fn codex_homes(
    user_home: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let mut homes = vec![default_codex_home()?];
    let swarm_homes = swarm::paths::codex_accounts_dir()?;
    for (folder, prefix) in [(user_home, ".codex-"), (swarm_homes.as_path(), "")] {
        if let Ok(entries) = std::fs::read_dir(folder) {
            homes.extend(
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.file_name().to_string_lossy().starts_with(prefix)
                            && entry.path().is_dir()
                    })
                    .map(|entry| entry.path()),
            );
        }
    }
    homes.sort();
    homes.dedup();
    Ok(homes)
}

/// `swarm hooks status --json`: whether swarm's own Codex and AGY state hooks and the guard are
/// set up. `swarm setup hooks` sets them up (ADR 0092). Claude's state hooks need no step, because
/// `swarm launch` passes them with `--settings`.
fn hooks(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
    let has_list = swarm::paths::guards_file()?.exists();
    let files = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["status", "--json"] => print_json(&serde_json::json!({
            "codex": files.homes.iter().all(|home| swarm::bus::codex_hooks_trusted(home, &codex)),
            "agy": swarm::bus::agy_hooks_set(&files.agy_hooks),
            "guard": files.guard_status(has_list),
        })),
        ["setup", ..] => Err("swarm: `swarm hooks setup` is now `swarm setup hooks`".into()),
        _ => Err(USAGE.into()),
    }
}

/// The files that `setup hooks` writes: each Codex home, AGY's `hooks.json`, and, for the guard,
/// each Claude `settings.json` and Codex `hooks.json`, one name per file.
struct HookFiles {
    homes: Vec<std::path::PathBuf>,
    agy_hooks: std::path::PathBuf,
    claude_settings: Vec<std::path::PathBuf>,
    codex_hooks: Vec<std::path::PathBuf>,
}

impl HookFiles {
    fn of(user_home: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let homes = codex_homes(user_home)?;
        let claude_settings = unique_targets(
            std::iter::once(user_home.join(".claude/settings.json")).chain(
                std::fs::read_dir(user_home.join(".claude/.profiles"))
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .map(|entry| entry.path().join("settings.json"))
                    .filter(|path| path.exists()),
            ),
        );
        let codex_hooks = unique_targets(homes.iter().map(|home| home.join("hooks.json")));
        Ok(Self {
            agy_hooks: user_home.join(".gemini/config/hooks.json"),
            homes,
            claude_settings,
            codex_hooks,
        })
    }

    /// Every item that `setup hooks` adds, with swarm's current text, so `managed` finds one that
    /// a build wrote before swarm kept a record (ADR 0042). The text names no build (ADR 0034), so
    /// an item equal to it is swarm's. A Codex key's table is swarm's own key, so it is created.
    fn items(&self) -> Vec<swarm::managed::Edit> {
        use swarm::managed::{Edit, Writer};
        let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
        let mut items = Vec::new();
        for home in &self.homes {
            // A hooks.json swarm cannot read finds no pre-record guard keys. That is safe: a
            // found item is only listed and reverted while present, and recorded keys still list.
            let hooks = std::fs::read_to_string(home.join("hooks.json")).unwrap_or_default();
            let guard = swarm::bus::codex_guard_trust(home, &hooks);
            let keys = codex.iter().map(|entry| (entry, Writer::HooksState));
            for ((key, hash), writer) in
                keys.chain(guard.iter().map(|entry| (entry, Writer::HooksGuard)))
            {
                items.push(Edit {
                    created: 1,
                    ..swarm::bus::codex_trust_edit(home, key, hash, writer)
                });
            }
        }
        let guarded = self.claude_settings.iter().map(|path| (path, "claude"));
        for (path, provider) in guarded.chain(self.codex_hooks.iter().map(|path| (path, "codex"))) {
            for event in swarm::guard::EVENTS {
                items.push(swarm::bus::guard_group_edit(path, provider, event));
            }
        }
        for name in ["swarm", "swarm-guard"] {
            items.push(swarm::bus::agy_group_edit(&self.agy_hooks, name));
        }
        items
    }
}

/// `swarm managed list [--json] | revert (<id>... | --all) [--plan [--json] | --digest <digest>]`:
/// each item swarm wrote outside its home, recorded or found, with its live state, and removing
/// items that still equal what swarm wrote (ADR 0042). Running `revert` is the consent, as running
/// `setup` is; `--plan` shows the diff first and `--digest` applies only that plan.
fn managed(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let store = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let found = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?.items();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["list", rest @ ..] if matches!(rest, [] | ["--json"]) => {
            let entries = swarm::managed::list(&store, &found)?;
            if rest.is_empty() {
                for entry in &entries {
                    println!(
                        "{}  {}  {}  {}  {}",
                        entry.edit.id(),
                        entry.state.name(),
                        swarm::managed::wire(&entry.edit.writer),
                        entry.edit.file.display(),
                        entry.edit.path.join(".")
                    );
                }
                return Ok(());
            }
            let entries: Vec<_> = entries.iter().map(swarm::managed::Entry::json).collect();
            print_json(&serde_json::json!({ "entries": entries }))
        }
        ["revert", rest @ ..] => {
            let split = rest
                .iter()
                .position(|arg| matches!(*arg, "--plan" | "--digest"))
                .unwrap_or(rest.len());
            let (targets, flags) = rest.split_at(split);
            let target = match targets {
                ["--all"] => swarm::managed::Target::All,
                [] => return Err(USAGE.into()),
                ids if ids.iter().all(|id| !id.starts_with('-')) => {
                    swarm::managed::Target::Ids(ids.iter().map(|id| id.to_string()).collect())
                }
                _ => return Err(USAGE.into()),
            };
            let plan = || swarm::managed::revert_plan(&store, &found, &target);
            match flags {
                ["--plan"] => {
                    print!(
                        "{}",
                        swarm::managed::plan_text(
                            &plan()?,
                            &format!("swarm managed revert {}", targets.join(" ")),
                            "Swarm has nothing to remove. No file changes."
                        )
                    );
                    Ok(())
                }
                ["--plan", "--json"] => print_json(&swarm::managed::plan_json(&plan()?)),
                [] | ["--digest", _] => {
                    if let Some(reason) = swarm::bus::revert_refusal(
                        env::var("SWARM_AGENT_ID").ok().as_deref(),
                        env::var("HERDR_AGENT_PANE").ok().as_deref(),
                    ) {
                        return Err(reason.into());
                    }
                    let lock = swarm::paths::trust_lock()?;
                    swarm::managed::with_lock(&lock, || {
                        let plans = plan()?;
                        if let ["--digest", digest] = flags
                            && swarm::managed::digest(&plans) != *digest
                        {
                            return Err("swarm: a managed file changed after the plan; check the plan again".into());
                        }
                        for path in swarm::managed::revert(&store, &plans)? {
                            println!("swarm: removed swarm's entries from {}", path.display());
                        }
                        Ok(())
                    })?;
                    Ok(())
                }
                _ => Err(USAGE.into()),
            }
        }
        _ => Err(USAGE.into()),
    }
}

impl HookFiles {
    /// Whether the guard registrations match the rule list: each one in place while the list
    /// exists, and none left once it is gone, because a registration with no list blocks every
    /// call.
    fn guard_status(&self, has_list: bool) -> bool {
        let files = || {
            let claude = self.claude_settings.iter().map(|path| (path, "claude"));
            claude.chain(self.codex_hooks.iter().map(|path| (path, "codex")))
        };
        if !has_list {
            return !swarm::bus::agy_guard_present(&self.agy_hooks)
                && files().all(|(path, provider)| !swarm::bus::guard_registered(path, provider));
        }
        files().all(|(path, provider)| {
            swarm::bus::guard_hooks_plan(path, provider)
                .is_ok_and(|plan| plan.conflicts.is_empty() && plan.after == plan.before)
        }) && self.homes.iter().all(|home| {
            let hooks = std::fs::read_to_string(home.join("hooks.json")).unwrap_or_default();
            let entries = swarm::bus::codex_guard_trust(home, &hooks);
            !entries.is_empty() && swarm::bus::codex_hooks_trusted(home, &entries)
        }) && swarm::bus::agy_guard_set(&self.agy_hooks)
    }

    /// Whether swarm's own Codex and AGY state hooks are set up.
    fn hooks_status(&self) -> bool {
        let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
        self.homes
            .iter()
            .all(|home| swarm::bus::codex_hooks_trusted(home, &codex))
            && swarm::bus::agy_hooks_set(&self.agy_hooks)
    }
}

/// The plan of every file `setup hooks` writes. A file that swarm cannot read or edit is a
/// conflict in the plan, not an error.
fn hook_plans(files: &HookFiles, has_list: bool) -> Vec<swarm::managed::FilePlan> {
    let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
    let unreadable = swarm::managed::FilePlan::unreadable;
    let guard_plan = |path: &std::path::PathBuf, provider: &str| {
        swarm::bus::guard_hooks_plan(path, provider)
            .unwrap_or_else(|error| unreadable(path.clone(), error))
    };
    let mut plans: Vec<_> = Vec::new();
    if has_list {
        plans.extend(
            files
                .claude_settings
                .iter()
                .map(|path| guard_plan(path, "claude")),
        );
        plans.extend(
            files
                .codex_hooks
                .iter()
                .map(|path| guard_plan(path, "codex")),
        );
    }
    let codex_plans: Vec<_> = files
        .homes
        .iter()
        .map(|home| {
            let mut guard = Vec::new();
            if has_list {
                // Trust the guard group where the planned hooks.json puts it.
                let target = canonical(&home.join("hooks.json"));
                let hooks = plans
                    .iter()
                    .find(|plan| canonical(&plan.path) == target)
                    .map_or("", |plan| plan.after.as_str());
                guard = swarm::bus::codex_guard_trust(home, hooks);
            }
            swarm::bus::codex_hook_plan(home, &codex, &guard)
                .unwrap_or_else(|error| unreadable(home.join("config.toml"), error))
        })
        .collect();
    plans.extend(codex_plans);
    plans.push(
        swarm::bus::agy_hook_plan(&files.agy_hooks, has_list)
            .unwrap_or_else(|error| unreadable(files.agy_hooks.clone(), error)),
    );
    plans
}

/// The items of `swarm setup`, in apply order (ADR 0092). The list may grow, so the app shows an
/// item it does not know by its name.
const SETUP_ITEMS: [&str; 4] = ["hooks", "trust", "plugin", "hud"];

/// One `swarm setup` plan: each file's plan in apply order, the item (`group`) of each, each item
/// part that was left out with its reason, and the commands the plugin and HUD items run.
struct SetupPlan {
    plans: Vec<swarm::managed::Plan>,
    selected: Vec<String>,
    groups: Vec<&'static str>,
    /// For each trust plan, how to plan that one file again when it is written (L-4), so a write
    /// under the trust lock reads no other file and runs no `git` (L-1).
    replans: Vec<Option<Replan>>,
    /// For each plan, whether it removes swarm's own items (`managed::revert`) instead of adding.
    removes: Vec<bool>,
    skipped: Vec<(&'static str, String)>,
    plugin: Option<PluginPlan>,
    /// The LaunchAgent text the HUD item writes, or None when the HUD already runs from it.
    hud: Option<String>,
    /// The HUDs that the login item did not start, which the HUD install stops.
    hud_strays: Vec<u32>,
}

type Replan = Box<dyn Fn() -> Result<swarm::managed::FilePlan, String>>;

impl SetupPlan {
    /// The plan of every pending write in `groups`. The trust group is the consent file set to
    /// `consent`, then the entries a launch in `cwd` would write, for a resuming seat when
    /// `resume`. A trust entry in a file that an earlier plan changes is planned on that plan's
    /// text, so the two apply one after the other.
    fn of(
        cwd: &std::path::Path,
        groups: &[&str],
        consent: &str,
        resume: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        use swarm::managed::FilePlan;
        let user_home = std::path::PathBuf::from(env_var("HOME")?);
        let files = HookFiles::of(&user_home)?;
        let mut setup = Self {
            plans: Vec::new(),
            selected: groups.iter().map(|group| group.to_string()).collect(),
            groups: Vec::new(),
            replans: Vec::new(),
            removes: Vec::new(),
            skipped: Vec::new(),
            plugin: None,
            hud: None,
            hud_strays: Vec::new(),
        };
        if groups.contains(&"hooks") {
            let has_list = swarm::paths::guards_file()?.exists();
            for plan in hook_plans(&files, has_list) {
                setup.push("hooks", plan);
            }
        }
        if groups.contains(&"trust") {
            let answer = consent.to_string();
            setup.push_trust(
                consent_plan(consent)?,
                Box::new(move || consent_plan(&answer).map_err(|error| error.to_string())),
            );
            match trust_target(cwd, &user_home) {
                Err(reason) => setup.skipped.push(("trust", reason)),
                Ok(target) => {
                    // A Codex home or Claude config swarm cannot edit is skipped with its reason,
                    // as a launch with no picked account does (`trust_each`).
                    let configs: Vec<_> = files
                        .homes
                        .iter()
                        .map(|home| home.join("config.toml"))
                        .collect();
                    let mut skipped = trust_each(&configs, false, |path| {
                        let earlier = setup
                            .plans
                            .iter()
                            .rev()
                            .filter_map(|plan| match plan {
                                swarm::managed::Plan::File(plan) => Some(plan),
                                _ => None,
                            })
                            .find(|plan| canonical(&plan.path) == canonical(path));
                        // A file the earlier plan cannot edit has no planned text to build on.
                        if let Some(conflict) = earlier.and_then(|plan| plan.conflicts.first()) {
                            return Err(conflict.found.clone());
                        }
                        let home = path.parent().unwrap_or(path).to_path_buf();
                        let plan = match earlier.map(|plan| plan.after.clone()) {
                            Some(text) => swarm::bus::codex_trust_plan_on(path, text, &target)?,
                            None => swarm::bus::codex_trust_plan(&home, &target)?,
                        };
                        if swarm::bus::codex_left_untrusted(&plan.before, &target) {
                            setup.skipped.push((
                                "trust",
                                format!(
                                    "{}: you marked this folder untrusted; the pane asks",
                                    path.display()
                                ),
                            ));
                        }
                        // At write time the hooks are on disk, so the replan reads the file.
                        let target = target.clone();
                        setup.push_trust(
                            plan,
                            Box::new(move || swarm::bus::codex_trust_plan(&home, &target)),
                        );
                        Ok(())
                    })?;
                    let settings = agy_settings(&user_home);
                    let plan = swarm::bus::agy_trust_plan(&settings, &target)
                        .unwrap_or_else(|error| FilePlan::unreadable(settings.clone(), error));
                    setup.push_trust(
                        plan,
                        Box::new(move || swarm::bus::agy_trust_plan(&settings, &target)),
                    );
                    // A seat's Claude runs from the pool under cwd, or in cwd when it resumes.
                    let flags = if resume {
                        vec!["--resume".into()]
                    } else {
                        Vec::new()
                    };
                    let (dir, _) = swarm::bus::claude_child("", cwd, &flags, &uuid::Uuid::nil());
                    skipped.extend(trust_each(&claude_configs(&user_home), false, |config| {
                        let plan = swarm::bus::claude_trust_plan(config, &dir)?;
                        let (config, dir) = (config.to_path_buf(), dir.clone());
                        setup.push_trust(
                            plan,
                            Box::new(move || swarm::bus::claude_trust_plan(&config, &dir)),
                        );
                        Ok(())
                    })?);
                    setup
                        .skipped
                        .extend(skipped.into_iter().map(|reason| ("trust", reason)));
                }
            }
        }
        if groups.contains(&"plugin") {
            // The skill links an older swarm made would list each kit skill twice (ADR 0092).
            let old_links = old_skill_links(&files)?;
            let leaving: Vec<_> = old_links
                .iter()
                .map(|plan| plan.path().to_path_buf())
                .collect();
            let plugin = plugin_plan(&user_home, &leaving)?;
            setup
                .skipped
                .extend(plugin.notes.iter().map(|note| ("plugin", note.clone())));
            for plan in old_links {
                if plan.conflicts().is_empty() {
                    setup.push("plugin", plan);
                    *setup.removes.last_mut().unwrap() = true;
                } else {
                    setup.skipped.push((
                        "plugin",
                        format!(
                            "{} is no longer swarm's link; swarm leaves it",
                            plan.path().display()
                        ),
                    ));
                }
            }
            setup.plugin = Some(plugin);
        }
        if groups.contains(&"hud") {
            let text = swarm::hud::planned()?;
            let now = swarm::hud::status(&user_home);
            let written = std::fs::read_to_string(swarm::hud::plist_path(&user_home)).ok();
            if !(now.running && written.as_ref() == Some(&text) && now.strays.is_empty()) {
                setup.hud = Some(text);
                setup.hud_strays = now.strays;
            }
        }
        Ok(setup)
    }

    fn push(&mut self, group: &'static str, plan: impl Into<swarm::managed::Plan>) {
        self.groups.push(group);
        self.plans.push(plan.into());
        self.replans.push(None);
        self.removes.push(false);
    }

    fn push_trust(&mut self, plan: swarm::managed::FilePlan, replan: Replan) {
        self.push("trust", plan);
        *self.replans.last_mut().unwrap() = Some(replan);
    }

    /// Each plan with its group.
    fn grouped(&self) -> impl Iterator<Item = (&'static str, &swarm::managed::Plan)> {
        self.groups.iter().copied().zip(&self.plans)
    }

    /// The digest that apply checks. Every running CLI rewrites a trust file such as
    /// `~/.claude.json`, so a trust plan's part is only each entry it adds and the value it
    /// replaces, and a change to one of those still changes the digest (L-4). Every other part is
    /// the file's whole text, as in `managed::digest`.
    fn digest(&self) -> String {
        use sha2::Digest;
        let mut digest = sha2::Sha256::new();
        digest.update(serde_json::to_vec(&self.selected).unwrap());
        digest.update([0]);
        for line in self
            .commands()
            .into_iter()
            .map(|(_, line)| line)
            .chain(self.hud.clone())
        {
            digest.update(line.as_bytes());
            digest.update([0]);
        }
        for (group, plan) in self.grouped() {
            let mut parts = vec![
                group.to_string(),
                plan.path().to_string_lossy().into_owned(),
            ];
            if group == "trust" {
                for edit in plan.edits() {
                    let before = edit.before.as_ref().map(serde_json::Value::to_string);
                    parts.extend([edit.id(), edit.wrote.to_string(), format!("{before:?}")]);
                }
            } else {
                parts.push(swarm::managed::digest(std::slice::from_ref(plan)));
            }
            for part in parts {
                digest.update(part.as_bytes());
                digest.update([0]);
            }
        }
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Each command line the plugin and HUD items run after the files, with its item.
    fn commands(&self) -> Vec<(&'static str, String)> {
        let mut lines: Vec<_> = self
            .plugin
            .iter()
            .flat_map(PluginPlan::lines)
            .map(|line| ("plugin", line))
            .collect();
        if self.hud.is_some() {
            if !self.hud_strays.is_empty() {
                let pids: Vec<_> = self.hud_strays.iter().map(u32::to_string).collect();
                lines.push((
                    "hud",
                    format!(
                        "run  kill {}, the usage HUDs that the login item did not start",
                        pids.join(" ")
                    ),
                ));
            }
            lines.push((
                "hud",
                "run  launchctl bootstrap, so the usage HUD starts at login".into(),
            ));
        }
        lines
    }

    /// The HUD's LaunchAgent diff, or "" without one.
    fn hud_diff(&self) -> String {
        let Some(text) = &self.hud else {
            return String::new();
        };
        let path = std::path::PathBuf::from(env_var("HOME").unwrap_or_default());
        let path = swarm::hud::plist_path(&path);
        let before = std::fs::read_to_string(&path).unwrap_or_default();
        swarm::diff::unified(&path.to_string_lossy(), &before, text)
    }

    fn is_empty(&self) -> bool {
        self.plans
            .iter()
            .all(|plan| !plan.changes() && plan.conflicts().is_empty())
            && self.plugin.is_none()
            && self.hud.is_none()
    }

    /// Each skipped part, each diff and command, then the conflicts, or else `apply`, the command
    /// that applies the plan, when this is only the plan.
    fn text(&self, apply: Option<&str>) -> String {
        let mut text: String = self
            .skipped
            .iter()
            .map(|(group, reason)| format!("skipped ({group}): {reason}\n"))
            .collect();
        if self.is_empty() {
            return text + "Swarm is already set up. Nothing to do.\n";
        }
        text.extend(self.plans.iter().map(swarm::managed::diff));
        text += &self.hud_diff();
        for (_, line) in self.commands() {
            text += &format!("{line}\n");
        }
        match (swarm::managed::conflicts_text(&self.plans), apply) {
            (Some(conflicts), _) => text + &format!("\n{conflicts}\n"),
            (None, Some(apply)) => {
                text + &format!("\nPlan only. Nothing changed. Run `{apply}` to apply.\n")
            }
            (None, None) => text,
        }
    }
}

/// The plugin item: the link `~/.swarm/kit` to this build's kit, and each CLI's install command.
struct PluginPlan {
    link: std::path::PathBuf,
    target: std::path::PathBuf,
    relink: bool,
    steps: Vec<swarm::plugin::Step>,
    notes: Vec<String>,
}

impl PluginPlan {
    fn lines(&self) -> Vec<String> {
        let link = self
            .relink
            .then(|| format!("link {} -> {}", self.link.display(), self.target.display()));
        link.into_iter()
            .chain(
                self.steps
                    .iter()
                    .map(|step| format!("run  {}", step.line())),
            )
            .collect()
    }

    fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.relink {
            // Only a link of swarm's own is replaced; a real folder there is the owner's.
            if self
                .link
                .symlink_metadata()
                .is_ok_and(|meta| meta.is_symlink())
            {
                std::fs::remove_file(&self.link)?;
            }
            std::os::unix::fs::symlink(&self.target, &self.link)
                .map_err(|error| format!("swarm: cannot link {}: {error}", self.link.display()))?;
        }
        for step in &self.steps {
            let mut command = std::process::Command::new(&step.argv[0]);
            command.args(&step.argv[1..]);
            if let Some(home) = &step.codex_home {
                command.env("CODEX_HOME", home);
            }
            let status = command.status()?;
            if !status.success() {
                return Err(format!(
                    "swarm: `{}` failed ({status}); the commands before it ran",
                    step.line()
                )
                .into());
            }
        }
        Ok(())
    }
}

/// The schema tag of `swarm doctor --json`, which `/api/doctor` checks.
const DOCTOR_SCHEMA: &str = "swarm.doctor.v1";

/// One `swarm.doctor.v1` row. `group` is the `swarm setup` item that fixes it, or None.
fn doctor_row(
    id: &str,
    title: &str,
    (state, reason): (&str, String),
    group: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({"id": id, "title": title, "state": state, "reason": reason, "group": group})
}

/// The rows of what `swarm setup` sets up. A row is `ok`, `setup` when `swarm setup <group>` can
/// fix it, or `broken` when the owner must act, with the reason.
fn setup_rows(
    home: &std::path::Path,
    files: &HookFiles,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let ok = |reason: &str| ("ok", reason.to_string());
    let setup = |reason: &str| ("setup", reason.to_string());
    let kit = swarm::paths::bundled("skills/kit").and_then(|kit| std::fs::canonicalize(kit).ok());
    let hud = swarm::hud::status(home);
    Ok(vec![
        doctor_row(
            "hooks",
            "Agent hooks",
            if files.hooks_status() {
                ok("Claude, Codex and AGY report to swarm")
            } else {
                setup("The agent CLIs do not report to swarm yet")
            },
            Some("hooks"),
        ),
        doctor_row(
            "guard",
            "Command guard",
            if files.guard_status(swarm::paths::guards_file()?.exists()) {
                ok("Agents ask before a guarded command")
            } else {
                setup("The guard hook is not in place")
            },
            Some("hooks"),
        ),
        doctor_row(
            "trust",
            "Launch consent",
            // Read here, not through trust_answer, so a file that cannot be read shows as broken.
            match read_consent(&swarm::paths::consent_file()?) {
                Err(error) => ("broken", error),
                Ok((_, value)) => {
                    match [STANDING, ASK].into_iter().find(|a| value[TRUST_KEY] == *a) {
                        Some(answer) => ("ok", format!("Answered: {answer}")),
                        None => setup("Not answered, so a launch asks for each folder"),
                    }
                }
            },
            Some("trust"),
        ),
        doctor_row(
            "plugin",
            "Plugin swarm",
            match kit {
                None => ("broken", "This build ships no kit".into()),
                Some(kit)
                    if std::fs::read_link(swarm::paths::root_dir()?.join("kit")).ok()
                        == Some(swarm::plugin::stable_kit(&kit)) =>
                {
                    ok("Installed from this build's kit")
                }
                Some(_) => setup("Not installed from this build's kit"),
            },
            Some("plugin"),
        ),
        doctor_row(
            "hud",
            "Usage HUD",
            match (hud.app.is_some(), hud.installed, hud.running) {
                (false, ..) => ("broken", "This build ships no HUD app".into()),
                (_, true, true) if !hud.strays.is_empty() => setup(&format!(
                    "Runs, with {} more {} that the login item did not start",
                    hud.strays.len(),
                    if hud.strays.len() == 1 {
                        "copy"
                    } else {
                        "copies"
                    }
                )),
                (_, true, true) => ok("Starts at login and runs"),
                (_, true, false) => ("broken", "Starts at login but does not run".into()),
                (_, false, _) => setup("Off"),
            },
            Some("hud"),
        ),
    ])
}

/// `swarm doctor [--json]`, also `swarm setup [--json]`: the rows of `setup_rows`, then each thing
/// swarm ships or runs. Once it can read HOME and the swarm home it exits 0, so the page can show
/// every row.
fn doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let json = match args {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => return Err("usage: swarm doctor [--json]".into()),
    };
    let home = std::path::PathBuf::from(env_var("HOME")?);
    let mut items = setup_rows(&home, &HookFiles::of(&home)?)?;
    items.push(doctor_row(
        "transcript",
        "Transcript tool",
        if env::var_os("SWARM_TRANSCRIPT_TOOL").is_some()
            || swarm::paths::bundled("transcript").is_some()
        {
            ("ok", "Chats can show their rows".into())
        } else {
            (
                "broken",
                "This build ships no transcript tool, so chats show no rows".into(),
            )
        },
        None,
    ));
    items.push(doctor_row("notices", "Notices", notices_state(), None));
    #[cfg(feature = "serve")]
    items.push(doctor_row("ui", "UI server", serve::ui_state(), None));
    if json {
        return print_json(&serde_json::json!({"schema": DOCTOR_SCHEMA, "items": items}));
    }
    for item in &items {
        println!(
            "{:<6} {}: {}",
            item["state"].as_str().unwrap_or_default(),
            item["title"].as_str().unwrap_or_default(),
            item["reason"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

/// The items whose row reads `setup`, in apply order: what `swarm setup all` sets up.
fn needed_items() -> Result<Vec<&'static str>, Box<dyn std::error::Error>> {
    let home = std::path::PathBuf::from(env_var("HOME")?);
    let rows = setup_rows(&home, &HookFiles::of(&home)?)?;
    Ok(SETUP_ITEMS
        .into_iter()
        .filter(|item| {
            rows.iter()
                .any(|row| row["group"] == *item && row["state"] == "setup")
        })
        .collect())
}

/// Whether the notice helper ships, and whether a Focus held one of its notices in the last hour.
/// A Focus lets through only the apps the owner allows, and an ad hoc signed helper cannot mark a
/// notice time-sensitive, so the owner must allow it in the Focus.
fn notices_state() -> (&'static str, String) {
    if notify_helper().is_none() {
        return ("broken", "This build ships no notice helper".into());
    }
    // The bundle id is in ui/Resources/SwarmNotify-Info.plist.
    let read = std::process::Command::new("/usr/bin/log")
        .args([
            "show",
            "--last",
            "1h",
            "--style",
            "compact",
            "--predicate",
            r#"process == "NotificationCenter" AND eventMessage CONTAINS "(io.github.priyanshuupadhyay.swarm.notify) muted by DND""#,
        ])
        .output();
    // A failed read must not read as no mute.
    let out = match read {
        Ok(out) if out.status.success() => out,
        failed => {
            let why = match failed {
                Ok(out) => String::from_utf8_lossy(&out.stderr).trim().to_string(),
                Err(error) => error.to_string(),
            };
            return (
                "broken",
                format!("Could not read the notice log, so a Focus mute is unknown: {why}"),
            );
        }
    };
    if String::from_utf8_lossy(&out.stdout).contains("muted by DND") {
        (
            "broken",
            "A Focus held swarm's notices in the last hour. Allow SwarmNotify in System Settings › Focus".into(),
        )
    } else {
        ("ok", "No Focus held a notice in the last hour".into())
    }
}

/// The plugin item: install the kit that ships beside `swarm` as the plugin `swarm` in each of
/// Claude, Codex, AGY and pi on PATH (ADR 0090). The commands go through each CLI's own plugin
/// command, so swarm records no managed edit for them.
/// A skill in `leaving` is a link this plan removes, so it is not named as the owner's own.
fn plugin_plan(
    user_home: &std::path::Path,
    leaving: &[std::path::PathBuf],
) -> Result<PluginPlan, Box<dyn std::error::Error>> {
    let kit = swarm::paths::bundled("skills/kit")
        .and_then(|kit| std::fs::canonicalize(kit).ok())
        .ok_or("swarm: this build ships no kit in libexec/skills/kit")?;
    if !kit.join(".claude-plugin/marketplace.json").is_file() {
        return Err(format!(
            "swarm: {} is not the swarm kit: it has no .claude-plugin/marketplace.json",
            kit.display()
        )
        .into());
    }
    let link = swarm::paths::root_dir()?.join("kit");
    let found: Vec<&str> = swarm::plugin::CLIS
        .into_iter()
        .filter(|cli| installed(cli))
        .collect();
    let mut codex = Vec::new();
    if found.contains(&"codex") {
        for home in codex_homes(user_home)? {
            if !home.is_dir() {
                continue;
            }
            let listed = std::process::Command::new("codex")
                .args(["plugin", "marketplace", "list", "--json"])
                .env("CODEX_HOME", &home)
                .output()?;
            codex.push(swarm::plugin::CodexHome {
                swarm_root: swarm::plugin::codex_swarm_root(&listed.stdout),
                home,
            });
        }
    }
    let steps = swarm::plugin::steps(&link, &kit, &found, &codex);
    if steps.is_empty() {
        return Err(
            "swarm: none of claude, codex, agy or pi is on PATH; nothing to install".into(),
        );
    }
    let target = swarm::plugin::stable_kit(&kit);
    let mut notes: Vec<String> = swarm::plugin::CLIS
        .iter()
        .filter(|cli| !found.contains(cli))
        .map(|cli| format!("{cli} is not on PATH"))
        .collect();
    let clashes = swarm::plugin::clashes(&kit, user_home);
    notes.extend(
        clashes
            .iter()
            .filter(|clash| !leaving.contains(clash))
            .map(|clash| {
                format!(
                    "{} is your own skill; it stays, and the CLI lists it beside the plugin's",
                    clash.display()
                )
            }),
    );
    Ok(PluginPlan {
        relink: std::fs::read_link(&link).ok().as_ref() != Some(&target),
        link,
        target,
        steps,
        notes,
    })
}

/// The plans that remove each skill link an older swarm's `skills` setup made and recorded.
fn old_skill_links(
    files: &HookFiles,
) -> Result<Vec<swarm::managed::Plan>, Box<dyn std::error::Error>> {
    use swarm::managed::{State, Target, Writer};
    let database = swarm::paths::sqlite_db()?;
    if !database.try_exists()? {
        return Ok(Vec::new());
    }
    // Read only, so a plan creates no database and changes no schema.
    let store = rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let found = files.items();
    let ids: Vec<String> = swarm::managed::list(&store, &found)?
        .into_iter()
        .filter(|entry| entry.edit.writer == Writer::Skills && entry.state == State::Present)
        .map(|entry| entry.edit.id())
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(swarm::managed::revert_plan(
        &store,
        &found,
        &Target::Ids(ids),
    )?)
}

/// The key of `~/.swarm/consent.json` that holds the owner's answer for launch folder trust, and
/// its two answers (ADR 0043). Reader and writer share these, so a typo cannot read as `ask`.
const TRUST_KEY: &str = "trust";
const STANDING: &str = "standing";
const ASK: &str = "ask";

/// `~/.swarm/consent.json` as its text ("" for a missing file) and its object.
fn read_consent(path: &std::path::Path) -> Result<(String, serde_json::Value), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((String::new(), serde_json::json!({})));
        }
        Err(error) => return Err(format!("swarm: cannot read {}: {error}", path.display())),
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) if value.is_object() => Ok((text, value)),
        _ => Err(format!("swarm: {} is not a JSON object", path.display())),
    }
}

/// The plan that sets the launch consent in `~/.swarm/consent.json` to `answer`, as a managed
/// edit, so Managed Changes lists it and its undo puts back the answer before. With none before,
/// the undo records `ask`, so the owner stays answered and the app asks once (ADR 0043); a first
/// `ask` itself undoes to none, because an edit that wrote its own before value never reads as
/// set back. A file swarm cannot read is a conflict.
fn consent_plan(answer: &str) -> Result<swarm::managed::FilePlan, Box<dyn std::error::Error>> {
    use swarm::managed::{Edit, FilePlan, Kind, Writer};
    let path = swarm::paths::consent_file()?;
    let (before, mut value) = match read_consent(&path) {
        Ok(read) => read,
        Err(error) => return Ok(FilePlan::unreadable(path, error)),
    };
    let found = value.get(TRUST_KEY).cloned();
    let mut plan = FilePlan {
        after: before.clone(),
        path,
        before,
        conflicts: Vec::new(),
        edits: Vec::new(),
    };
    if found.as_ref().and_then(serde_json::Value::as_str) != Some(answer) {
        value[TRUST_KEY] = answer.into();
        plan.after = serde_json::to_string_pretty(&value)? + "\n";
        plan.edits.push(Edit {
            before: found
                .or_else(|| Some(ASK.into()))
                .filter(|before| *before != answer),
            ..Edit::new(
                Writer::LaunchTrust,
                &plan.path,
                Kind::JsonKey,
                &[TRUST_KEY],
                answer.into(),
            )
        });
    }
    Ok(plan)
}

/// `swarm setup [--json] | status --json | <items> [--plan [--json] | --digest <digest> | --yes]
/// [--cwd <dir>] [--consent <standing|ask>] [--resume]` (ADR 0092). With no item it lists each
/// row and writes nothing. `<items>` is a comma list of `SETUP_ITEMS` or `all`, the items that
/// need setup. Every write is one plan with one digest (ADR 0043): `hooks`, as the hook files;
/// `trust`, the launch consent (`--consent`, by default the recorded answer, else standing) and the
/// trust entries a launch in `--cwd` would write, for a resuming seat with `--resume`; `plugin`,
/// the old skill links and each CLI's install command; `hud`, the LaunchAgent. A terminal run shows
/// the plan and asks; `--yes` or the plan's digest is the owner's consent, so a child pane may plan
/// but not apply.
fn setup(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => {
            doctor(&[])?;
            let needed = needed_items()?;
            if needed.is_empty() {
                println!("\nNothing to set up.");
            } else {
                println!(
                    "\nSet up:    swarm setup {}   (or swarm setup all)",
                    needed.join(",")
                );
            }
            println!(
                "Items:     {}\nPlan only: swarm setup <item> --plan",
                SETUP_ITEMS.join(", ")
            );
            return Ok(());
        }
        ["--json"] => return doctor(&["--json".into()]),
        ["status", "--json"] => {
            let files = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?;
            let has_list = swarm::paths::guards_file()?.exists();
            return print_json(&serde_json::json!({
                "hooks": files.hooks_status(),
                "guard": files.guard_status(has_list),
                // The owner answered, standing or ask, so the app does not ask again.
                "trust": trust_answer().is_some(),
            }));
        }
        _ => {}
    }
    if args.contains(&"--only") {
        return Err(
            "swarm: `--only` is gone; name the items, as in `swarm setup hooks,trust`".into(),
        );
    }
    let [list, flags @ ..] = args.as_slice() else {
        return Err(USAGE.into());
    };
    let (mut plan, mut json, mut yes, mut digest, mut cwd) = (false, false, false, None, None);
    let (mut consent, mut resume) = (None, false);
    let mut rest = flags.iter().copied();
    while let Some(arg) = rest.next() {
        match arg {
            "--plan" => plan = true,
            "--json" => json = true,
            "--yes" => yes = true,
            "--resume" => resume = true,
            "--digest" | "--cwd" | "--consent" => {
                let value = Some(rest.next().ok_or(USAGE)?);
                match arg {
                    "--digest" => digest = value,
                    "--cwd" => cwd = value,
                    _ => consent = value,
                }
            }
            _ => return Err(USAGE.into()),
        }
    }
    if list.starts_with('-')
        || (json && !plan)
        || [plan, yes, digest.is_some()]
            .iter()
            .filter(|set| **set)
            .count()
            > 1
        || consent.is_some_and(|answer| ![STANDING, ASK].contains(&answer))
    {
        return Err(USAGE.into());
    }
    let groups: Vec<&str> = if *list == "all" {
        needed_items()?
    } else {
        let named: Vec<&str> = list.split(',').collect();
        if let Some(unknown) = named.iter().find(|item| !SETUP_ITEMS.contains(item)) {
            return Err(format!(
                "swarm: unknown setup item {unknown}; items: {}, or all",
                SETUP_ITEMS.join(", ")
            )
            .into());
        }
        SETUP_ITEMS
            .into_iter()
            .filter(|item| named.contains(item))
            .collect()
    };
    if groups.is_empty() && !plan {
        println!("swarm: everything is set up. Nothing to do.");
        return Ok(());
    }
    // No --consent keeps the owner's answer, so the approve command a seat prints under `ask`
    // approves the folder and not standing consent; with no answer yet, setup offers standing.
    // An apply reads it under the trust lock, so an answer recorded while it waits is kept (L-4).
    let answer = || consent.unwrap_or_else(|| trust_answer().unwrap_or(STANDING));
    let dir = cwd.map_or_else(env::current_dir, |dir| Ok(dir.into()))?;
    let dir = std::fs::canonicalize(&dir)
        .map_err(|error| format!("swarm: bad --cwd {}: {error}", dir.display()))?;
    let refusal = swarm::bus::setup_refusal(
        env::var("SWARM_AGENT_ID").ok().as_deref(),
        env::var("HERDR_AGENT_PANE").ok().as_deref(),
    );
    if digest.is_none() {
        let setup = SetupPlan::of(&dir, &groups, answer(), resume)?;
        let plan_digest = setup.digest();
        if json {
            let group_of = |value: serde_json::Value, group: &str| {
                let mut value = value;
                value["group"] = group.into();
                value
            };
            let mut files: Vec<_> = setup
                .grouped()
                .filter(|(_, plan)| plan.changes())
                .map(|(group, plan)| {
                    serde_json::json!({
                        "group": group,
                        "path": plan.path().to_string_lossy(),
                        "diff": swarm::managed::diff(plan),
                    })
                })
                .collect();
            if let Some(diff) = Some(setup.hud_diff()).filter(|diff| !diff.is_empty()) {
                let home = std::path::PathBuf::from(env_var("HOME")?);
                files.push(serde_json::json!({
                    "group": "hud",
                    "path": swarm::hud::plist_path(&home).to_string_lossy(),
                    "diff": diff,
                }));
            }
            return print_json(&serde_json::json!({
                "digest": plan_digest,
                "consent": answer(),
                "items": groups,
                "files": files,
                "commands": setup.commands().iter()
                    .map(|(group, line)| serde_json::json!({"group": group, "line": line}))
                    .collect::<Vec<_>>(),
                "conflicts": setup.grouped()
                    .flat_map(|(group, plan)| plan.conflicts().iter().map(move |conflict| (group, conflict)))
                    .map(|(group, conflict)| group_of(serde_json::json!(conflict), group))
                    .collect::<Vec<_>>(),
                "skipped": setup.skipped.iter()
                    .map(|(group, reason)| serde_json::json!({"group": group, "reason": reason}))
                    .collect::<Vec<_>>(),
            }));
        }
        // The resolved items, not `all`, so the digest still names the plan shown.
        let mut apply = format!("swarm setup {} --digest {plan_digest}", groups.join(","));
        if let Some(cwd) = cwd {
            apply += &format!(" --cwd {}", swarm::adapter::shell_line(&[cwd.into()]));
        }
        if let Some(consent) = consent {
            apply += &format!(" --consent {consent}");
        }
        if resume {
            apply += " --resume";
        }
        if !plan && let Some(conflicts) = swarm::managed::conflicts_text(&setup.plans) {
            return Err(conflicts.into());
        }
        print!("{}", setup.text(plan.then_some(&*apply)));
        if plan || setup.is_empty() {
            return Ok(());
        }
        if let Some(reason) = refusal {
            return Err(reason.into());
        }
        if !yes {
            use std::io::{BufRead, IsTerminal, Write};
            if !std::io::stdin().is_terminal() {
                return Err(format!(
                    "swarm: nothing changed; run `swarm setup {list} --yes` to apply"
                )
                .into());
            }
            print!("Apply this plan? [y/N] ");
            std::io::stdout().flush()?;
            let mut reply = String::new();
            std::io::stdin().lock().read_line(&mut reply)?;
            if !matches!(reply.trim(), "y" | "Y" | "yes") {
                println!("swarm: nothing changed");
                return Ok(());
            }
        }
        // `--yes` applies what is pending when the lock is free, as the plan was only shown; an
        // answer to the question applies only the plan it answered.
        let shown = (!yes).then_some(&*plan_digest);
        return apply_setup(&dir, &groups, answer, resume, shown);
    }
    if let Some(reason) = refusal {
        return Err(reason.into());
    }
    apply_setup(&dir, &groups, answer, resume, digest)
}

/// Plan `groups` again under the trust lock and apply it, only while it still has `digest` when
/// one is given: the files first, then the plugin's commands and the HUD's LaunchAgent once the
/// lock is free.
fn apply_setup<'a>(
    dir: &std::path::Path,
    groups: &[&str],
    answer: impl Fn() -> &'a str,
    resume: bool,
    digest: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Swarm makes no write that it cannot record (ADR 0042), so the store opens first.
    let store = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let setup = swarm::managed::with_lock(&swarm::paths::trust_lock()?, || {
        let setup =
            SetupPlan::of(dir, groups, answer(), resume).map_err(|error| error.to_string())?;
        // A retry after a timeout finds nothing to do, and that is not a failure.
        if setup.is_empty() {
            return Ok(None);
        }
        if digest.is_some_and(|digest| setup.digest() != digest) {
            return Err("swarm: a file changed after the plan; check the plan again".into());
        }
        if let Some(conflicts) = swarm::managed::conflicts_text(&setup.plans) {
            return Err(conflicts);
        }
        // A running CLI may rewrite its trust file at any moment, such as `~/.claude.json`, and
        // the digest covers only the trust entries, so each trust file is planned again and
        // written as launch's `write_trust` does (L-4).
        for (((group, plan), replan), removes) in
            setup.grouped().zip(&setup.replans).zip(&setup.removes)
        {
            let written = match replan {
                Some(replan) => swarm::bus::write_trust(&store, replan)
                    .map(|written| written.into_iter().map(|plan| plan.path).collect()),
                None if *removes => swarm::managed::revert(&store, std::slice::from_ref(plan)),
                None => swarm::managed::apply(&store, std::slice::from_ref(plan)),
            };
            match written {
                Ok(changed) => {
                    for path in changed {
                        let verb = if *removes { "removed" } else { "wrote" };
                        println!("swarm: {group}: {verb} {}", path.display());
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "{error}\nswarm: stopped; each file named above is written and recorded, see `swarm managed list`"
                    ));
                }
            }
        }
        Ok(Some(setup))
    })?;
    let Some(setup) = setup else {
        println!("swarm: already set up. Nothing to do.");
        return Ok(());
    };
    if let Some(plugin) = &setup.plugin {
        plugin.run()?;
        println!("swarm: plugin: the plugin swarm is installed; a new session loads it");
    }
    if setup.hud.is_some() {
        let status = swarm::hud::install(&std::path::PathBuf::from(env_var("HOME")?))?;
        let running = if status.running {
            "runs"
        } else {
            "does not run yet"
        };
        println!("swarm: hud: the usage HUD starts at login and {running}");
    }
    Ok(())
}

/// A path as its link target, so two names of one file are one file. Two plans of one missing
/// file both write the same text, which `write_text` accepts.
fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Each path whose link target no earlier path shares, because two plans of one file would each
/// find it changed by the other at apply.
fn unique_targets(paths: impl Iterator<Item = std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    paths.filter(|path| seen.insert(canonical(path))).collect()
}

/// Folder trust in each Codex home or Claude config a pane may read. With no account picked, one
/// that swarm cannot edit is skipped, and its path and reason returned, so one broken spare
/// profile does not stop every launch or setup; the one of a picked account must take the entry.
fn trust_each(
    paths: &[std::path::PathBuf],
    picked: bool,
    mut ensure: impl FnMut(&std::path::Path) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    let mut skipped = Vec::new();
    for path in paths {
        match ensure(path) {
            Err(error) if !picked => skipped.push(format!("{}: {error}", path.display())),
            result => result?,
        }
    }
    Ok(skipped)
}

/// What a launch may do with the trust entries its pane needs (ADR 0043).
#[derive(Clone, Copy, PartialEq)]
enum TrustConsent {
    /// The owner gave standing consent: write each entry and name it.
    Standing,
    /// A chair in the folder the owner picked, with no standing consent: write each entry for
    /// this folder, and show its diff, so the write is never silent (owner answer I1).
    Picked,
    /// No consent: write nothing; show each pending diff and the command that approves it.
    Ask,
}

/// The owner's answer for launch folder trust in `~/.swarm/consent.json`: `standing`, `ask`, or
/// None for no answer. A file swarm cannot read is no answer (R4), and is named.
fn trust_answer() -> Option<&'static str> {
    let path = swarm::paths::consent_file().ok()?;
    match read_consent(&path) {
        Ok((_, value)) => [STANDING, ASK]
            .into_iter()
            .find(|answer| value[TRUST_KEY] == *answer),
        Err(error) => {
            eprintln!("{error}; folder trust has no consent");
            None
        }
    }
}

/// The owner's standing consent for launch folder trust. Anything else is no consent (R4).
fn standing_consent() -> bool {
    trust_answer() == Some(STANDING)
}

/// The trust entries one launch needs, and what it may do with them.
struct LaunchTrust<'a> {
    store: &'a rusqlite::Connection,
    consent: TrustConsent,
    /// Each file must take the entry, as a picked account's or AGY's only one does (see
    /// `trust_each`).
    required: bool,
    /// The launch's folder, for the command that approves a pending entry.
    cwd: &'a std::path::Path,
}

impl LaunchTrust<'_> {
    /// Plan `dir` trusted for `provider` in each of `files`. With consent, write each plan
    /// through the managed-edits module under the trust lock, and print the bare
    /// `trusted <provider> <dir>` line the app reads. With none, write nothing, and print the bare
    /// `trust-pending <provider> <dir>` line, each diff, and the command that approves it.
    fn run(
        &self,
        provider: &str,
        dir: &std::path::Path,
        files: &[std::path::PathBuf],
        plan: fn(&std::path::Path, &std::path::Path) -> Result<swarm::managed::FilePlan, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.consent == TrustConsent::Ask {
            let mut diffs = String::new();
            for file in files {
                match plan(file, dir) {
                    Ok(plan) => diffs += &swarm::managed::diff(&plan.into()),
                    Err(error) => eprintln!("swarm: skipped {}: {error}", file.display()),
                }
            }
            if !diffs.is_empty() {
                eprintln!("trust-pending {provider} {}", dir.display());
                eprint!("{diffs}");
                eprintln!(
                    "swarm: no consent for folder trust, so swarm wrote nothing; the pane asks instead."
                );
                // A Claude seat runs in cwd only when it resumes (`claude_child`).
                let resume = if provider == "claude" && dir == self.cwd {
                    " --resume"
                } else {
                    ""
                };
                eprintln!(
                    "swarm: approve with `swarm setup trust --plan --cwd {}{resume}`, then the `swarm setup trust --digest …` it prints.",
                    swarm::adapter::shell_line(&[self.cwd.to_string_lossy().into_owned()])
                );
            }
            return Ok(());
        }
        let mut written = Vec::new();
        let skipped = swarm::managed::with_lock(&swarm::paths::trust_lock()?, || {
            trust_each(files, self.required, |file| {
                written.extend(swarm::bus::write_trust(self.store, || plan(file, dir))?);
                Ok(())
            })
        })?;
        for reason in skipped {
            eprintln!("swarm: skipped {reason}");
        }
        for plan in &written {
            if self.consent == TrustConsent::Picked {
                eprint!("{}", swarm::managed::diff(&plan.clone().into()));
            }
            eprintln!(
                "swarm: trusted {} for {provider} in {}",
                dir.display(),
                plan.path.display()
            );
        }
        if !written.is_empty() {
            eprintln!("trusted {provider} {}", dir.display());
        }
        Ok(())
    }
}

/// AGY's settings file, which holds its folder trust.
fn agy_settings(user_home: &std::path::Path) -> std::path::PathBuf {
    user_home.join(".gemini/antigravity-cli/settings.json")
}

/// Native and legacy Claude config files for the setup trust plan.
fn claude_configs(user_home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut configs = vec![user_home.join(".claude.json")];
    if let Ok(profiles) = std::fs::read_dir(user_home.join(".claude/.profiles")) {
        configs.extend(
            profiles
                .filter_map(Result::ok)
                // Hidden dirs hold helper data, not account config files.
                .filter(|entry| {
                    !entry.file_name().to_string_lossy().starts_with('.') && entry.path().is_dir()
                })
                .map(|entry| entry.path().join(".claude.json")),
        );
    }
    configs
}

fn claude_chair_log(id: &str) -> Option<std::path::PathBuf> {
    let config = env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".claude"))
        })?;
    std::fs::read_dir(config.join("projects"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(format!("{id}.jsonl")))
        .find(|path| path.is_file())
}

fn codex_chair_log(id: &str, days: &[String; 3]) -> Option<std::path::PathBuf> {
    let home = env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".codex"))
        })?;
    let suffix = format!("-{id}.jsonl");
    days.iter()
        .filter_map(|day| std::fs::read_dir(home.join("sessions").join(day)).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(&suffix))
        })
}

/// A pi session file, `<agent dir>/sessions/--<cwd>--/<time>_<id>.jsonl`. Pi gives each tool
/// command the file of its own session, so that comes first.
fn pi_chair_log(id: &str) -> Option<std::path::PathBuf> {
    let suffix = format!("_{id}.jsonl");
    // The name test comes first, so a scan of every session file stats only the match.
    let named = |path: &std::path::Path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(&suffix))
            && path.is_file()
    };
    if let Some(file) = env::var_os("PI_SESSION_FILE").map(std::path::PathBuf::from)
        && named(&file)
    {
        return Some(file);
    }
    let agent = env::var_os("PI_CODING_AGENT_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".pi/agent"))
        })?;
    std::fs::read_dir(agent.join("sessions"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|dir| std::fs::read_dir(dir.path()).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| named(path))
}

/// The app reads a live chat's log from its session row, and a pi chair's file is known when
/// the chair is named, so it is recorded then.
fn record_pi_log(
    connection: &rusqlite::Connection,
    session: &str,
    chair: Option<(&str, &str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(("pi", id)) = chair
        && let Some(log) = pi_chair_log(id)
    {
        swarm::store::set_chair_log(connection, session, &log)?;
    }
    Ok(())
}

fn resolved_chair_log(row: &swarm::store::SessionRow) -> Option<std::path::PathBuf> {
    if let Some(path) = row.chair_log.as_deref().map(std::path::PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let provider = row.chair_provider.as_deref()?;
    let id = row.chair_id.as_deref().filter(|id| valid_chair_id(id))?;
    match provider {
        "claude" => claude_chair_log(id),
        "codex" => codex_chair_log(id, &row.chair_days),
        "pi" => pi_chair_log(id),
        _ => None,
    }
}

/// An agent's chat log, or the same Codex rollout under `archived_sessions/` once Codex has moved
/// it there from `sessions/<y>/<m>/<d>/`.
fn resolved_agent_log(log: String) -> String {
    let path = std::path::Path::new(&log);
    if path.is_file() {
        return log;
    }
    path.ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "sessions"))
        .and_then(|sessions| {
            Some(
                sessions
                    .parent()?
                    .join("archived_sessions")
                    .join(path.file_name()?),
            )
        })
        .filter(|archived| archived.is_file())
        .map_or(log, |archived| archived.to_string_lossy().into_owned())
}

struct SpawnOptions<'a> {
    provider: Option<&'a str>,
    account: Option<&'a str>,
    resolved_account: Option<&'a swarm::profiles::Account>,
    command: &'a [String],
    runner: Option<&'a swarm::config::Runner>,
}

fn parse_spawn_options(args: &[String]) -> Result<SpawnOptions<'_>, String> {
    let mut provider = None;
    let mut account = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--" if provider.is_none() || account.is_some() => {
                return Ok(SpawnOptions {
                    provider,
                    account,
                    resolved_account: None,
                    command: &args[index + 1..],
                    runner: None,
                });
            }
            "--provider" if provider.is_none() && index + 1 < args.len() => {
                provider = Some(args[index + 1].as_str());
                index += 2;
            }
            "--account" if account.is_none() && index + 1 < args.len() => {
                account = Some(args[index + 1].as_str());
                index += 2;
            }
            _ => return Err(USAGE.to_string()),
        }
    }
    if provider.is_some() && account.is_none() {
        return Err(USAGE.to_string());
    }
    Ok(SpawnOptions {
        provider,
        account,
        resolved_account: None,
        command: &[],
        runner: None,
    })
}

/// Refuse a sender or recipient that is not an agent in the session, and a recipient without a
/// pane, then store the message and ring it. The bell is a hint (R9), so a ring failure only warns.
#[allow(clippy::too_many_arguments)]
fn deliver(
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
fn deliver_with(
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
    // The message is stored now, so a failed ring mark only warns: the row stays unrung, a later
    // re-ring rings it, and a caller such as `report` does not send it a second time.
    let mark = || -> Result<Option<i64>, Box<dyn std::error::Error>> {
        if swarm::store::has_rung_unread(connection, session_id, &recipient)? {
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
                &recipient,
                &pane,
                &[seq],
                rung_at,
                proof,
            );
        }
        Ok(None) => {}
        Err(error) => eprintln!("swarm: message {seq} stored but not rung: {error}"),
    }
    Ok(seq)
}

fn ack(
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

fn add_agent(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    agent_id: &str,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pane = if role == "orchestrator" {
        Some(swarm::adapter::load(root, adapter_name)?.run("self", &[])?)
    } else {
        None
    };
    let transaction = connection.unchecked_transaction()?;
    // A chair registers again each time its CLI starts, from a pane that may be new. Any other
    // clash is still an error, so a second agent cannot take an existing agent's name.
    let rejoins = pane.is_some()
        && swarm::store::agents(&transaction, session_id)?
            .iter()
            .any(|agent| agent.id == agent_id && agent.role == role);
    if !rejoins {
        swarm::store::add_agent(&transaction, session_id, agent_id, role)?;
    }
    // An orchestrator that cannot say which pane it is in is refused, because the alternative is
    // a chat that starts, looks healthy, and drops the first message somebody types into it.
    if let Some(pane) = pane {
        swarm::store::set_adapter(&transaction, session_id, adapter_name)?;
        swarm::store::set_pane(&transaction, session_id, agent_id, &pane)?;
    }
    transaction.commit()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn register_spawned_pane(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent_id: &str,
    role: &str,
    provider: Option<&str>,
    vars: &[(&str, &str)],
    launch: Option<(&swarm::config::Runner, Option<&str>)>,
) -> Result<String, Box<dyn std::error::Error>> {
    swarm::store::add_agent(connection, session_id, agent_id, role)?;
    if let Some(provider) = provider {
        swarm::store::set_provider(connection, session_id, agent_id, provider)?;
    }
    let pane = match adapter.run("spawn", vars) {
        Ok(pane) => pane,
        Err(error) => {
            swarm::store::remove_agent(connection, session_id, agent_id)?;
            return Err(error);
        }
    };
    swarm::store::set_pane(connection, session_id, agent_id, &pane)?;
    if let Some((runner, account)) = launch {
        swarm::store::set_launch(connection, session_id, agent_id, role, runner, account)?;
    }
    Ok(pane)
}

fn list_models(provider: &str) -> Result<swarm::providers::ModelList, Box<dyn std::error::Error>> {
    let models = Provider::parse(provider)
        .ok_or_else(|| format!("swarm: unsupported model provider {provider}"))?
        .models(|cli, args| tool_stdout(cli, args).map_err(|error| error.to_string()))?;
    if models.is_empty() {
        return Err(format!("swarm: {provider} returned no models").into());
    }
    Ok(swarm::providers::ModelList {
        provider: provider.into(),
        models,
    })
}

/// Where `launch` may pre-trust Codex and AGY for `cwd`; see `swarm::bus::trust_target`.
fn trust_target(
    cwd: &std::path::Path,
    user_home: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let git_root = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            std::fs::canonicalize(String::from_utf8_lossy(&output.stdout).trim()).ok()
        });
    let uid = std::os::unix::fs::MetadataExt::uid(
        &std::fs::metadata(user_home).map_err(|error| error.to_string())?,
    );
    let swarm_home = swarm::paths::home().map_err(|error| error.to_string())?;
    let scratch_roots: Vec<_> = [
        std::path::PathBuf::from("/private/tmp/councils"),
        std::path::PathBuf::from(format!("/private/tmp/claude-{uid}")),
        std::path::Path::new(&swarm_home).join(".swarm/ws"),
        user_home.join("swarm/workspaces.noindex"),
    ]
    .iter()
    .filter_map(|root| std::fs::canonicalize(root).ok())
    .collect();
    swarm::bus::trust_target(
        cwd,
        git_root.as_deref(),
        &std::fs::canonicalize(user_home).map_err(|error| error.to_string())?,
        &scratch_roots,
    )
}

fn spawn_agent(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    agent_id: &str,
    role: &str,
    options: SpawnOptions<'_>,
    account_deadline: std::time::Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    // Before any pane, file, or bus work: the id names a pane, a bus row, and a run script.
    if !swarm::bus::valid_agent_id(agent_id) {
        return Err(format!("swarm: bad agent id {agent_id}").into());
    }
    let account = if let Some(account) = options.resolved_account {
        Some(account.clone())
    } else if let Some(requested) = options.account {
        let provider = match options.provider {
            Some(provider) => provider.to_string(),
            None => swarm::config::load()
                .and_then(|(config, _)| {
                    config
                        .profile(role)
                        .map(|profile| profile.runners[0].provider.id().to_string())
                })
                .map_err(|error| format!("swarm: {error}"))?,
        };
        let accounts = load_accounts(&provider, account_deadline)?;
        Some(
            swarm::profiles::resolve_account(&accounts, requested)
                .map_err(|error| format!("swarm: {error}"))?
                .clone(),
        )
    } else {
        None
    };
    let session_id = session_id()?;
    let provider = options
        .provider
        .or_else(|| options.command.first().map(String::as_str))
        .and_then(Provider::parse);
    // Warn, do not refuse: an unknown name usually means the role config or the CLI is stale,
    // and the pane shows the real error if the model does not run.
    if let (Some(provider), Some(model)) = (provider, swarm::bus::command_model(options.command)) {
        let account_env = account
            .as_ref()
            .map(|account| account.env.clone())
            .unwrap_or_default();
        if provider
            .catalog(&account_env)
            .is_some_and(|catalog| !provider.model_known(&catalog, model))
        {
            eprintln!(
                "swarm: warning: the installed {} CLI does not list model {model}; the role config may need an update",
                provider.id()
            );
        }
    }
    let adapter = swarm::adapter::load(root, &adapter_name())?;
    let session = session_id.to_string();
    let home = swarm::paths::home()?;
    let current_adapter = adapter_name();
    let vars = [
        ("session_id", session.as_str()),
        ("agent_id", agent_id),
        ("home", home.as_str()),
        ("adapter", current_adapter.as_str()),
    ];
    let pane = register_spawned_pane(
        connection,
        &adapter,
        &session_id,
        agent_id,
        role,
        provider.map(Provider::id),
        &vars,
        options.runner.map(|runner| (runner, options.account)),
    )?;
    if !options.command.is_empty() {
        // When run through the pane's `runs/<session>/bin/swarm` link, current_exe is that link,
        // and linking to it would make the link point at itself.
        let exe = std::fs::canonicalize(env::current_exe()?)?;
        // The agent's own `swarm inbox` and `swarm finish` must run this binary, not an older
        // `swarm` on the pane's PATH, which refuses a database another build made. The pane gets
        // a dir with only a link to it, since this binary's own dir (such as ~/.cargo/bin) holds
        // other tools that would shadow the owner's.
        let bin = swarm_bin(root, &session_id, &exe)?;
        let path = format!(
            "export PATH={}:\"$PATH\"; ",
            swarm::adapter::shell_line(&[bin.to_string_lossy().into_owned()])
        );
        let exe = exe.to_string_lossy().into_owned();
        let hook = swarm::adapter::shell_line(&[exe, "exited".into()]);
        let child = if account.is_some()
            || (options.runner.is_some() && provider.is_some_and(Provider::has_accounts))
        {
            // `env` finds the CLI on PATH, so a pane function with the CLI's name never runs. It
            // is one command with no shell syntax, so a fish pane runs it too, and the pane's own
            // variables stay as they were.
            let mut args = vec!["env".to_string()];
            for key in provider
                .map(|provider| provider.account_env_keys())
                .unwrap_or_default()
            {
                args.extend(["-u".into(), key.to_string()]);
            }
            args.push("--".into());
            args.extend(
                account
                    .iter()
                    .flat_map(|account| &account.env)
                    .map(|(key, value)| format!("{key}={value}")),
            );
            args.extend_from_slice(options.command);
            swarm::adapter::shell_line(&args)
        } else {
            swarm::adapter::shell_line(options.command)
        };
        let line = script_line(root, &session_id, agent_id, &format!("{path}{child}"))?;
        adapter.run(
            "ring",
            &[("pane", &pane), ("text", &format!("{line}; {hook}"))],
        )?;
    }
    println!("{pane}");
    if let Some(account) = account {
        eprintln!("account {}", account.name);
    }
    // The app shows this model until the agent's own log reports one; a new chat writes no log
    // before its first message.
    if let Some(model) = swarm::bus::command_model(options.command) {
        eprintln!("model {model}");
    }
    Ok(())
}

/// Point `runs/<session>/bin/swarm` at `exe` and return that dir.
fn swarm_bin(
    root: &std::path::Path,
    session_id: &str,
    exe: &std::path::Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use std::os::unix::fs::DirBuilderExt;
    valid_session_id(session_id)?;
    let dir = root.join(format!("runs/{session_id}/bin"));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    // A new link renamed over the old one, so a pane starting now never finds no `swarm`.
    let tmp = dir.join(format!(".swarm.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(exe, &tmp)?;
    std::fs::rename(&tmp, dir.join("swarm"))?;
    Ok(dir)
}

/// Save `command` as `runs/<session>/<agent>.sh` and return the short line that sources it.
/// A ring types into a new pane before its shell is ready, and the terminal then keeps only the
/// first 1024 bytes of the line, so a long argv never reaches the shell whole. The pane's own
/// shell sources the file, so a `swarm spawn` command can be one of its functions; `env` starts a
/// provider CLI.
fn script_line(
    root: &std::path::Path,
    session_id: &str,
    agent_id: &str,
    command: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    // Both ids become path parts; a `../` in either would write and source a file outside runs/.
    if !swarm::bus::valid_agent_id(agent_id) {
        return Err(format!("swarm: bad agent id {agent_id}").into());
    }
    valid_session_id(session_id)?;
    let dir = root.join(format!("runs/{session_id}"));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    // The line can carry an account's env, so only the owner reads it.
    let path = dir.join(format!("{agent_id}.sh"));
    let tmp = dir.join(format!(".{agent_id}.sh.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| {
            std::io::Write::write_all(&mut file, format!("{command}\n").as_bytes())
        })
        .and_then(|()| std::fs::rename(&tmp, &path));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("swarm: cannot write {}: {error}", path.display()).into());
    }
    Ok(format!(
        ". {}",
        swarm::adapter::shell_line(&[path.to_string_lossy().into_owned()])
    ))
}

fn attach(agent_id: &str) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let pane = swarm::store::require_pane(&connection, &session_id()?, agent_id)?;
    swarm::adapter::load(&root, &adapter_name())?.attach(&[("pane", &pane)])
}

fn focus(session_id: &str, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let adapter = swarm::store::adapter_of(&connection, session_id)?
        .ok_or("swarm: session has no adapter")?;
    let adapter = swarm::adapter::load(&root, &adapter)?;
    // Unsupported hosts report their capability, rather than pretending the pane is closed.
    if adapter.focus.is_none() {
        return Err(format!("swarm: {} has no focus", adapter.name).into());
    }
    let try_pane = |agent| -> Result<(), Box<dyn std::error::Error>> {
        let pane =
            swarm::store::pane_of(&connection, session_id, agent)?.ok_or("swarm: pane closed")?;
        adapter.run("focus", &[("pane", &pane)])?;
        Ok(())
    };
    if try_pane(agent_id).is_err() {
        let chair = swarm::store::orchestrator_of(&connection, session_id)
            .map_err(|_| "swarm: pane closed")?;
        try_pane(&chair).map_err(|_| "swarm: pane closed")?;
    }
    activate_client(&adapter);
    Ok(())
}

fn activate_client(adapter: &swarm::adapter::Adapter) {
    if adapter.client.is_none() {
        return;
    }
    let Ok(client) = adapter.run("client", &[]) else {
        return;
    };
    let Ok(pid) = client.trim().parse::<u32>() else {
        return;
    };
    if let Some(app) = client_app(pid) {
        // ponytail: with several Ghostty windows the last-focused one rises; a per-window pick
        // needs AX. Add it if this bites.
        let _ = std::process::Command::new("open").arg(app).status();
    }
}

fn client_app(mut pid: u32) -> Option<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    while pid > 1 && seen.insert(pid) {
        let command = std::process::Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !command.status.success() {
            return None;
        }
        let command = String::from_utf8(command.stdout).ok()?;
        let path = command.trim();
        if let Some(end) = path.find(".app/Contents/MacOS/") {
            return Some(std::path::PathBuf::from(&path[..end + 4]));
        }
        let parent = std::process::Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !parent.status.success() {
            return None;
        }
        pid = String::from_utf8(parent.stdout).ok()?.trim().parse().ok()?;
    }
    None
}

/// A child ended without `swarm finish`: forget its pane, then send a fallback summary in its
/// name and queue a summarize job unless it already sent one (R19).
fn report_dead(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: &str,
    child: &str,
    note: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    swarm::store::clear_pane(connection, session_id, child)?;
    if !swarm::store::has_summary(connection, session_id, child)? {
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
        swarm::store::enqueue_job(connection, session_id, child, "summarize")?;
    }
    Ok(())
}

/// Ring `agent` again when its unseen messages are due for another ring. When their last ring
/// starts no turn and `agent` is the chair, returns the report line `unconfirmed <agent> <seq>`; a
/// child's lost message is reported by `report_lost`.
fn rering_if_due(
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
fn lost_body(agent: &str, seq: i64) -> String {
    format!(
        "message {seq} to {agent} was not delivered: no turn started after {} rings",
        swarm::store::MAX_RINGS
    )
}

/// A message about the chair would ring the pane that just lost two rings, so the chair's own lost
/// message is only stderr and this line (ADR 0041).
fn chair_lost(chair: &str, seq: i64) -> String {
    eprintln!("swarm: {}", lost_body(chair, seq));
    format!("unconfirmed {chair} {seq}")
}

/// Send `subject`'s report `kind` to the chair, once: the trigger `message_report` refuses a
/// second one. Returns whether this call stored it. A failed send only warns, so the pass that
/// found the report goes on, and the next pass finds the report again.
#[allow(clippy::too_many_arguments)]
fn report(
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
fn sweep_once(
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

/// Whether the chair's pane reads idle with no question on it. A ring's Enter typed over a
/// question, such as a permission prompt, would answer it.
fn chair_idle(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let chair = swarm::store::orchestrator_of(connection, session_id)?;
    let Some(pane) = swarm::store::pane_of(connection, session_id, &chair)? else {
        return Ok(false);
    };
    let provider = swarm::store::provider_of(connection, session_id, &chair)?;
    let vars = [("pane", pane.as_str())];
    let capture = || adapter.capture_within(&vars, std::time::Duration::from_secs(1));
    let screen = match adapter.screen {
        Some(_) => adapter.screen(&vars, std::time::Duration::from_secs(1)),
        None => capture(),
    };
    Ok(screen.is_some_and(|screen| {
        swarm::screen::whole_prompt(&screen, capture).is_none()
            && matches!(
                swarm::screen::read_pane(provider.as_deref(), &screen, capture),
                Some((swarm::screen::ScreenState::Idle, _))
            )
    }))
}

/// Settle each ring that no caller waited for: the listing's, and one whose caller ended in its
/// wait. Its proof is a hook, the screen, or a read now; with none, it is unconfirmed once its deadline has
/// passed (ADR 0041). Adds a line for the chair's own lost message to `lines` at once, so the line
/// stays when a later ring fails. With `Proof::Later`, the listing's pass, the chair's own last ring
/// stays for the sweep.
fn settle_rings(
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
fn report_lost(
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
fn report_stalls(
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

/// Feed the agent's captured log to the summarizer shell command and return its output.
fn summarize_log(
    log: &std::path::Path,
    summarizer: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let input = std::fs::File::open(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(summarizer)
        .stdin(input)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "summarizer failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn process_summary_job(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    summarizer: &str,
    job_id: i64,
) -> Result<String, Box<dyn std::error::Error>> {
    let (session, agent, kind, attempts) = swarm::store::job(connection, job_id)?;
    if kind != "summarize" {
        swarm::store::park_job(connection, job_id)?;
        return Ok(format!("parked {job_id} unknown kind {kind}"));
    }
    let log = root.join(format!("runs/{session}/{agent}.log"));
    let result = (|| {
        let summary = summarize_log(&log, summarizer)?;
        let orchestrator = swarm::store::orchestrator_of(connection, &session)?;
        deliver(
            connection,
            root,
            adapter_name,
            &session,
            &agent,
            &orchestrator,
            "summary",
            &summary,
        )?;
        std::fs::remove_file(&log)?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })();
    match result {
        Ok(()) => {
            swarm::store::finish_job(connection, job_id)?;
            Ok(format!("done {job_id}"))
        }
        Err(error) if attempts < 3 => {
            swarm::store::release_job(connection, job_id, 30)?;
            Ok(format!("retry {job_id}: {error}"))
        }
        Err(error) => {
            swarm::store::park_job(connection, job_id)?;
            Ok(format!("parked {job_id}: {error}"))
        }
    }
}

fn set_chair_for_caller(
    connection: &rusqlite::Connection,
    session_id: &str,
    caller: &str,
    chair: Option<(&str, &str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    if swarm::store::orchestrator_of(connection, session_id)? != caller {
        return Err("swarm: only the orchestrator can set the session chair".into());
    }
    swarm::store::set_chair(connection, session_id, chair)
}

#[derive(serde::Serialize)]
struct AgentListOutput {
    agents: Vec<swarm::bus::Agent>,
    attachable: bool,
}

fn list_agents(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session_id: &str,
    adapter: &swarm::adapter::Adapter,
) -> Result<AgentListOutput, Box<dyn std::error::Error>> {
    let listing = read_pane_list(adapter)?;
    list_agents_from(connection, root, session_id, adapter, listing)
}

/// The adapter's `list` output, or None when it fails before the deadline.
fn read_pane_list(
    adapter: &swarm::adapter::Adapter,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match adapter.run("list", &[]) {
        Ok(listing) => Ok(Some(listing)),
        Err(error) => {
            adapter.check_deadline()?;
            eprintln!("swarm: {}", error.to_string().replace(['\r', '\n'], " "));
            Ok(None)
        }
    }
}

fn list_agents_from(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session_id: &str,
    adapter: &swarm::adapter::Adapter,
    listing: Option<String>,
) -> Result<AgentListOutput, Box<dyn std::error::Error>> {
    let rows = swarm::store::agents(connection, session_id)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let alive: Vec<Option<bool>> = rows
        .iter()
        .map(|row| {
            row.pane.as_deref().and_then(|pane| {
                listing
                    .as_deref()
                    .map(|list| swarm::adapter::listing_has_pane(list, pane))
            })
        })
        .collect();
    // A session with no chair yet still lists; it has no one to report to.
    let chair = swarm::store::orchestrator_of(connection, session_id).ok();
    // A chair that `session new` registered names its CLI only on the session, so its screen is
    // read with the provider `provider_of` resolves. The row's own provider stays as listed.
    let chair_provider = chair.as_ref().and_then(|chair| {
        swarm::store::provider_of(connection, session_id, chair)
            .ok()
            .flatten()
    });
    let providers: Vec<_> = rows
        .iter()
        .map(|row| {
            if Some(&row.id) == chair.as_ref() {
                chair_provider.clone()
            } else {
                row.provider.clone()
            }
        })
        .collect();
    // The screen check (ADR 0021) reads each live pane once per listing, all at the same
    // time, so six agents cost about one capture.
    type ScreenRead = (
        swarm::screen::ScreenState,
        Option<String>,
        Option<swarm::screen::Prompt>,
    );
    let screens: Vec<Option<ScreenRead>> = std::thread::scope(|scope| {
        let reads: Vec<_> = rows
            .iter()
            .zip(&alive)
            .zip(&providers)
            .map(|((row, alive), provider)| {
                let target = match (alive, row.pane.as_deref()) {
                    (Some(true), Some(pane)) => Some((pane, provider.as_deref())),
                    _ => None,
                };
                scope.spawn(move || {
                    let (pane, provider) = target?;
                    let output =
                        adapter.screen(&[("pane", pane)], std::time::Duration::from_millis(300))?;
                    let prompt = swarm::screen::whole_prompt(&output, || {
                        adapter.capture_within(
                            &[("pane", pane)],
                            std::time::Duration::from_millis(300),
                        )
                    });
                    let (state, detail) = swarm::screen::read_pane(provider, &output, || {
                        adapter.capture_within(
                            &[("pane", pane)],
                            std::time::Duration::from_millis(300),
                        )
                    })?;
                    Some((state, detail, prompt))
                })
            })
            .collect();
        reads
            .into_iter()
            .map(|read| read.join().ok().flatten())
            .collect()
    });
    // All reads must finish in time; a due ring below can run past their deadline.
    adapter.check_deadline()?;
    let mut chair_row_idle = false;
    let mut agents = Vec::new();
    for (((mut row, alive), screen), provider) in
        rows.into_iter().zip(alive).zip(screens).zip(providers)
    {
        let (screen, detail, prompt) = match screen {
            Some((screen, detail, prompt)) => (Some(screen), detail, prompt),
            None => (None, None, None),
        };
        let (state, write) = if swarm::screen::held_by_send(row.sent_at, row.state_at, screen, now)
        {
            (Some("working".to_string()), None)
        } else {
            swarm::screen::resolve(
                provider.as_deref(),
                row.state.as_deref(),
                row.state_at,
                row.state_source.as_deref(),
                screen,
                now,
            )
        };
        // The app runs no `swarm sweep`, and a ring typed while the CLI still starts is lost,
        // so the listing it polls rings a due message again once the pane shows it idle. A
        // fresh hook outranks the screen, so a turn it reports gets no ring typed into it.
        let idle = screen == Some(swarm::screen::ScreenState::Idle)
            && prompt.is_none()
            && !matches!(state.as_deref(), Some("working" | "waiting"));
        if Some(&row.id) == chair.as_ref() {
            chair_row_idle = idle;
        }
        if idle
            && let Some(pane) = row.pane.as_deref()
            && let Err(error) = rering_if_due(
                connection,
                root,
                adapter,
                session_id,
                &row.id,
                pane,
                Proof::Later,
            )
        {
            eprintln!("swarm: {error}");
        }
        if let Some(seen) = write {
            let detail = detail.filter(|_| seen == "failed");
            match swarm::store::set_screen_state(
                connection,
                session_id,
                &row.id,
                seen,
                detail.as_deref(),
                now,
                (
                    row.state.as_deref(),
                    row.state_source.as_deref(),
                    row.state_at,
                    row.state_kind.as_deref(),
                ),
            ) {
                Ok(true) => {
                    if seen == "waiting"
                        && row.state.as_deref() != Some("waiting")
                        && let Err(error) = notice_waiting(connection, session_id, &row.id, None)
                    {
                        eprintln!("swarm: {error}");
                    }
                    (row.state_at, row.state_source, row.state_detail) =
                        (Some(now), Some("screen".into()), detail);
                    row.state_kind = None;
                }
                // A newer hook report landed after this listing read the row; it stands,
                // and the next listing shows it.
                Ok(false) => {}
                Err(error) => eprintln!("swarm: {error}"),
            }
        }
        let state_kind = row
            .state_kind
            .filter(|_| state.as_deref() == Some("waiting"));
        agents.push(swarm::bus::Agent {
            id: row.id,
            role: row.role,
            pane: row.pane,
            provider: row.provider,
            created_at: row.created_at,
            alive,
            state,
            state_at_s: row.state_at,
            state_source: row.state_source,
            state_detail: row.state_detail,
            state_kind,
            log: row.log.map(resolved_agent_log),
            prompt,
            profile: row.profile,
            runner: row.runner,
            model: row.model,
            effort: row.effort,
            account: row.account,
            cost_usd: row.cost_usd,
            tokens: row.tokens,
        });
    }
    // The app runs no `swarm sweep`, so its listing also settles rings and reports lost messages
    // and stalls, after its state writes. The app kills the listing at 20 s, so none of its rings
    // waits for proof; the next pass settles them. A report rings the chair's stored pane, which a
    // restarted tmux or Herdr server can give to another pane, and its Enter would answer a
    // question on the chair's screen, so it waits for a listing that shows the chair's pane idle,
    // the same check as the re-ring above. Rings and settling take seconds, so the chair's screen
    // is read again right before the report.
    if let Err(error) = settle_rings(
        connection,
        root,
        adapter,
        session_id,
        Proof::Later,
        &mut Vec::new(),
    ) {
        eprintln!("swarm: {error}");
    }
    let chair_still_idle = chair_row_idle
        && chair_idle(connection, adapter, session_id).unwrap_or_else(|error| {
            eprintln!("swarm: {error}");
            false
        });
    if chair_still_idle {
        if let Err(error) = report_lost(connection, root, &adapter.name, session_id, Proof::Later) {
            eprintln!("swarm: {error}");
        }
        if let Err(error) = report_stalls(connection, root, &adapter.name, session_id, Proof::Later)
        {
            eprintln!("swarm: {error}");
        }
    }
    Ok(AgentListOutput {
        agents,
        attachable: adapter.attach.is_some(),
    })
}

fn all_agent_listings(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    budget: std::time::Duration,
) -> Result<std::collections::BTreeMap<String, AgentListOutput>, Box<dyn std::error::Error>> {
    let mut listings = std::collections::BTreeMap::new();
    let deadline = std::time::Instant::now() + budget;
    let sessions: Vec<_> = swarm::store::sessions(connection)?
        .into_iter()
        .filter(|session| {
            session.agents > 0
                && session
                    .adapter
                    .as_deref()
                    .is_some_and(|name| !name.trim().is_empty())
        })
        .collect();
    fn adapter_of(session: &swarm::store::SessionRow) -> &str {
        session.adapter.as_deref().expect("filtered adapter").trim()
    }
    // Every shipped adapter's `list` names all panes, not one session's, so it runs once per
    // adapter, with the env of the first session that uses it. An Err leaves those sessions out.
    let mut pane_lists: Vec<(&str, Result<Option<String>, String>)> = Vec::new();
    for session in &sessions {
        if !pane_lists
            .iter()
            .any(|(name, _)| *name == adapter_of(session))
        {
            pane_lists.push((adapter_of(session), Ok(None)));
        }
    }
    let adapters = pane_lists.len();
    for (index, (name, panes)) in pane_lists.iter_mut().enumerate() {
        let first = sessions
            .iter()
            .find(|s| adapter_of(s) == *name)
            .expect("listed adapter");
        *panes = swarm::adapter::load(root, name)
            .and_then(|mut adapter| {
                adapter.session_id = Some(first.id.clone());
                let now = std::time::Instant::now();
                adapter.deadline =
                    Some(now + deadline.saturating_duration_since(now) / (adapters - index) as u32);
                read_pane_list(&adapter)
            })
            .map_err(|error| error.to_string());
    }
    let panes_of = |session: &swarm::store::SessionRow| {
        &pane_lists
            .iter()
            .find(|(name, _)| *name == adapter_of(session))
            .expect("listed adapter")
            .1
    };
    // Only a session with a live pane reads its screen, so only those share the time left. A
    // session whose panes are unknown counts as live.
    let mut live = Vec::with_capacity(sessions.len());
    for session in &sessions {
        let rows = swarm::store::agents(connection, &session.id)?;
        live.push(match panes_of(session) {
            Ok(Some(panes)) => rows.iter().any(|row| {
                row.pane
                    .as_deref()
                    .is_some_and(|pane| swarm::adapter::listing_has_pane(panes, pane))
            }),
            _ => true,
        });
    }
    for (index, session) in sessions.iter().enumerate() {
        let sharers = if live[index] {
            live[index..].iter().filter(|live| **live).count()
        } else {
            sessions.len() - index
        };
        let listing = panes_of(session)
            .clone()
            .map_err(Into::into)
            .and_then(|panes| {
                let mut adapter = swarm::adapter::load(root, adapter_of(session))?;
                adapter.session_id = Some(session.id.clone());
                let now = std::time::Instant::now();
                adapter.deadline =
                    Some(now + deadline.saturating_duration_since(now) / sharers as u32);
                list_agents_from(connection, root, &session.id, &adapter, panes)
            });
        match listing {
            Ok(listing) => {
                listings.insert(session.id.clone(), listing);
            }
            Err(error) => eprintln!("swarm: {}: {error}", session.id),
        }
    }
    Ok(listings)
}

const ACCOUNT_PICK_TIMEOUT_SECONDS: u64 = 2;
/// A usage refresh before an automatic pick: one CLI start and one quota read per account.
const USAGE_PICK_REFRESH_TIMEOUT_SECONDS: u64 = 8;
// Leave time for CLI output before SwarmCLIProfileSource kills its process at 20 seconds.
const NATIVE_READ_TIMEOUT_SECONDS: u64 = 18;
const LOGIN_ADAPTER_TIMEOUT_SECONDS: u64 = 18;
const LOGIN_PANE_CLOSE_TIMEOUT_SECONDS: u64 = 1;

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if let [cmd, rest @ ..] = args
        && cmd == "run"
    {
        return run_cli(rest);
    }

    #[cfg(feature = "serve")]
    match args.first().map(String::as_str) {
        Some("serve") => return serve::run(&args[1..]),
        Some("ui") => return serve::ui(&args[1..]),
        _ => {}
    }
    if let [cmd, session_id, agent_id] = args
        && cmd == "focus"
    {
        return focus(session_id, agent_id);
    }
    // The commit this binary was built from, which is the only way a machine can tell the bus it
    // runs from the bus the repository states. `build.rs` stamps it.
    if let [flag] = args
        && (flag == "--version" || flag == "-V")
    {
        println!(
            "swarm {} {} {}",
            env!("CARGO_PKG_VERSION"),
            env!("SWARM_BUILD_COMMIT"),
            env!("SWARM_BUILD_BRANCH")
        );
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("init") {
        return init();
    }
    if args.first().map(String::as_str) == Some("setup") {
        return setup(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("doctor") {
        return doctor(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("hooks") {
        return hooks(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("managed") {
        return managed(&args[1..]);
    }
    if let [cmd, json] = args
        && cmd == "roles"
        && json == "--json"
    {
        return print_json(&swarm::config::listing().map_err(|error| format!("swarm: {error}"))?);
    }
    if let [cmd, title, rest @ ..] = args
        && cmd == "notify"
    {
        let body = match rest {
            [] => "",
            [flag, body] if flag == "--body" => body,
            _ => return Err(USAGE.into()),
        };
        if title.trim().is_empty() {
            return Err(USAGE.into());
        }
        // ADR 0044: a worker reports to the chair, and swarm itself notifies when it waits.
        if swarm::host::is_worker(|name| env::var(name).ok()) {
            return Err(
                "swarm: only the chair notifies the owner; send it to the orchestrator with swarm send"
                    .into(),
            );
        }
        let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
        let caller = identity().ok().filter(|(session, agent)| {
            swarm::store::orchestrator_of(&connection, session).is_ok_and(|chair| chair == *agent)
        });
        swarm::store::add_notice(
            &connection,
            caller.as_ref().map(|(session, _)| session.as_str()),
            caller.as_ref().map(|(_, agent)| agent.as_str()),
            title,
            body,
        )?;
        start_notify_helper();
        return Ok(());
    }
    if let [cmd, verb, rest @ ..] = args
        && cmd == "hud"
    {
        let json = match rest {
            [] => false,
            [flag] if flag == "--json" => true,
            _ => return Err(USAGE.into()),
        };
        // ADR 0087: the HUD starts at login only after the owner opts in, never from a worker.
        if verb != "status" && swarm::host::is_worker(|name| env::var(name).ok()) {
            return Err("swarm: only the owner turns the usage HUD on or off".into());
        }
        let home = std::path::PathBuf::from(env::var("HOME")?);
        let status = match verb.as_str() {
            "install" => return Err("swarm: `swarm hud install` is now `swarm setup hud`".into()),
            "uninstall" => swarm::hud::uninstall(&home)?,
            "status" => swarm::hud::status(&home),
            _ => return Err(USAGE.into()),
        };
        if json {
            return print_json(&Versioned {
                schema: swarm::hud::HUD_SCHEMA,
                body: &status,
            });
        }
        println!(
            "usage HUD: {}, {}",
            if status.installed {
                "starts at login"
            } else {
                "not installed"
            },
            if status.running {
                "running"
            } else {
                "not running"
            }
        );
        return Ok(());
    }
    if let [cmd, json] = args
        && cmd == "notices"
        && json == "--json"
    {
        let mut connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
        return print_json(&swarm::store::take_notices(&mut connection)?);
    }
    if let [cmd, due] = args
        && cmd == "notices"
        && due == "--due"
    {
        return notices_due();
    }
    if let [cmd] = args
        && cmd == "herdr-split"
    {
        let orchestrator = env_var("HERDR_PANE_ID")?;
        let cwd = env::current_dir()?;
        println!(
            "{}",
            swarm::herdr::split(&orchestrator, &cwd.to_string_lossy(), |name| env::var(name)
                .ok())?
        );
        return Ok(());
    }
    if let [cmd, flag, provider] = args
        && cmd == "host-context"
        && flag == "--provider"
        // pi has no SessionStart hook to call this; `launch` passes it the context (ADR 0086).
        && Provider::parse(provider).is_some_and(|provider| provider != Provider::Pi)
    {
        let mut payload = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut payload)?;
        swarm::host::contain_worker_history(provider, &payload, &env_var("HOME")?);
        print!(
            "{}",
            swarm::host::render(
                provider,
                swarm::host::context(provider, |name| env::var(name).ok()).as_deref()
            )
        );
        return Ok(());
    }
    if let [cmd, sub, role, rest @ ..] = args
        && cmd == "roles"
        && sub == "get"
    {
        let provider = match rest {
            [] => None,
            [flag, provider] if flag == "--provider" => Some(provider.as_str()),
            _ => return Err(USAGE.into()),
        };
        let (resolved, _) = resolve_role(role, provider, None, false)?;
        println!("{}", serde_json::to_string_pretty(&resolved)?);
        return Ok(());
    }
    if let [cmd, sub, json] = args
        && cmd == "roles"
        && sub == "check"
        && json == "--json"
    {
        let (config, _) = swarm::config::load().map_err(|error| format!("swarm: {error}"))?;
        let probe = Probe::new(&config, None);
        let profiles: Vec<swarm::config::ProfileCheck> = config
            .profiles
            .iter()
            .map(|profile| swarm::config::ProfileCheck {
                name: profile.name.clone(),
                selection: swarm::config::select(profile, None, |runner| probe.check(runner)),
            })
            .collect();
        return print_json(&serde_json::json!({ "profiles": profiles }));
    }
    if let [cmd, sub, flag, revision, text] = args
        && cmd == "roles"
        && sub == "save"
        && flag == "--revision"
    {
        let profile: swarm::config::Profile =
            serde_json::from_str(text).map_err(|error| format!("swarm: profile JSON: {error}"))?;
        let revision =
            swarm::config::save(profile, revision).map_err(|error| format!("swarm: {error}"))?;
        return print_json(&serde_json::json!({ "revision": revision }));
    }
    if let [cmd, sub, rest @ ..] = args
        && cmd == "roles"
        && matches!(
            sub.as_str(),
            "new" | "rename" | "copy" | "delete" | "reset" | "set-min-usage"
        )
    {
        use swarm::config::ProfileAction;
        let (action, revision) = match (sub.as_str(), rest) {
            ("new", [name, flag, revision]) if flag == "--revision" => {
                (ProfileAction::New(name), revision)
            }
            ("rename", [from, to, flag, revision]) if flag == "--revision" => {
                (ProfileAction::Rename(from, to), revision)
            }
            ("copy", [from, to, flag, revision]) if flag == "--revision" => {
                (ProfileAction::Copy(from, to), revision)
            }
            ("delete", [name, flag, revision]) if flag == "--revision" => {
                (ProfileAction::Delete(name), revision)
            }
            ("reset", [flag, revision]) if flag == "--revision" => (ProfileAction::Reset, revision),
            ("set-min-usage", [pct, flag, revision]) if flag == "--revision" => (
                ProfileAction::SetMinUsage(
                    pct.parse()
                        .map_err(|_| "swarm: min_usage_left_pct must be 0 to 100.")?,
                ),
                revision,
            ),
            _ => return Err(USAGE.into()),
        };
        let revision = swarm::config::profile_action(action, revision)
            .map_err(|error| format!("swarm: {error}"))?;
        return print_json(&serde_json::json!({ "revision": revision }));
    }
    if let [cmd, json] = args
        && cmd == "providers"
        && json == "--json"
    {
        return print_json(&swarm::providers::info_list());
    }
    if let [cmd, provider_flag, provider, json] = args
        && cmd == "models"
        && provider_flag == "--provider"
        && json == "--json"
    {
        return print_json(&list_models(provider)?);
    }
    if let [cmd, sub, rest @ ..] = args
        && cmd == "accounts"
        && sub == "login"
    {
        return account_login(rest);
    }
    if let [cmd, sub, rest @ ..] = args
        && cmd == "accounts"
        && sub == "pick"
    {
        return account_pick(rest);
    }
    if let [cmd, sub, revision_flag, revision, json] = args
        && cmd == "accounts"
        && sub == "reset"
        && revision_flag == "--revision"
        && json == "--json"
    {
        return print_json(&serde_json::json!({"revision":swarm::accounts::reset(revision)?}));
    }
    if let [cmd, provider_flag, provider, json] = args
        && cmd == "accounts"
        && provider_flag == "--provider"
        && json == "--json"
    {
        let provider = Provider::parse(provider).ok_or("swarm: unknown provider")?;
        return print_json(&Versioned {
            schema: swarm::profiles::ACCOUNTS_SCHEMA,
            body: &swarm::profiles::native::load(
                provider,
                swarm::profiles::native::deadline(NATIVE_READ_TIMEOUT_SECONDS),
            )?,
        });
    }
    if let [cmd, json] = args
        && cmd == "working"
        && json == "--json"
    {
        // The usage HUD polls this every few seconds, so it reads the store and no pane.
        let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
        return print_json(&serde_json::json!({
            "schema": "swarm.working.v1",
            "working": swarm::store::agents_by_provider(&connection, "working")?,
            "waiting": swarm::store::agents_by_provider(&connection, "waiting")?,
        }));
    }
    if let [cmd, json] = args
        && cmd == "usage"
        && json == "--json"
    {
        return print_json(&Versioned {
            schema: swarm::usage::USAGE_SCHEMA,
            body: &swarm::usage::read(swarm::profiles::native::deadline(
                NATIVE_READ_TIMEOUT_SECONDS,
            ))?,
        });
    }
    if let [cmd, flag, format] = args
        && cmd == "usage"
        && flag == "--format"
    {
        if format != "openusage" {
            return Err(format!(
                "swarm: unknown usage format {format}; the one format is openusage"
            )
            .into());
        }
        let usage = swarm::usage::read(swarm::profiles::native::deadline(
            NATIVE_READ_TIMEOUT_SECONDS,
        ))?;
        return print_json(&swarm::usage::openusage(
            &usage,
            swarm::usage::now_seconds(),
        ));
    }
    if let [cmd, refresh, provider_flag, provider, json] = args
        && cmd == "usage"
        && refresh == "--refresh"
        && provider_flag == "--provider"
        && json == "--json"
    {
        let deadline = swarm::profiles::native::deadline(NATIVE_READ_TIMEOUT_SECONDS);
        return print_json(&Versioned {
            schema: swarm::usage::USAGE_SCHEMA,
            body: &match provider.as_str() {
                "codex" => swarm::usage::refresh_codex(deadline)?,
                "claude" => swarm::usage::refresh_claude(deadline)?,
                _ => return Err("swarm: unknown usage provider".into()),
            },
        });
    }
    let root = swarm::paths::root_dir()?;
    if let [cmd, sub, name] = args
        && cmd == "adapter"
        && sub == "check"
    {
        swarm::adapter::load(&root, name)?;
        // Which verbs a deployed file takes over, because a parse alone says nothing: the file
        // that stopped every chat parsed cleanly and answered `self` with the wrong line.
        let path = root.join("adapters").join(format!("{name}.conf"));
        match std::fs::read_to_string(&path) {
            Ok(text) => match swarm::adapter::overrides(name, &text)?.join(" ") {
                named if named.is_empty() => println!("ok {name}: shipped"),
                named => println!("ok {name}: shipped, with {named} from {}", path.display()),
            },
            Err(_) => println!("ok {name}: shipped"),
        }
        return Ok(());
    }
    // A sandbox that denies the Herdr socket usually denies this database too, so the three calls
    // that start an orchestrator run check the socket first, and the caller reads that reason, not
    // a database error.
    let starts_a_run = match args {
        [cmd, ..] if cmd == "launch" => true,
        [cmd, sub, ..] => (cmd == "session" && sub == "new") || (cmd == "agent" && sub == "add"),
        _ => false,
    };
    if starts_a_run && adapter_name() == "herdr" {
        let status = run_tool("herdr", &["status"])?;
        if let Some(reason) = swarm::bus::socket_refusal(&String::from_utf8_lossy(&status.stderr)) {
            return Err(reason.into());
        }
    }
    let mut connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let session_new = match args {
        [cmd, sub, talk_mode] if cmd == "session" && sub == "new" => {
            Some((talk_mode.as_str(), env_chair()))
        }
        [cmd, sub, talk_mode, flag, value]
            if cmd == "session" && sub == "new" && flag == "--chair" =>
        {
            Some((
                talk_mode.as_str(),
                parse_chair(value)?.map(|(provider, id)| (provider.to_string(), id.to_string())),
            ))
        }
        _ => None,
    };
    if let Some((talk_mode, chair)) = session_new {
        let adapter = adapter_name();
        let session = swarm::store::create_session(
            &connection,
            talk_mode,
            &env::current_dir()?,
            chair
                .as_ref()
                .map(|(provider, id)| (provider.as_str(), id.as_str())),
            Some(&adapter),
        )?;
        record_pi_log(
            &connection,
            &session,
            chair.as_ref().map(|(p, id)| (p.as_str(), id.as_str())),
        )?;
        // A chair in a Herdr, Superset, or tmux pane is registered here, so it needs no ids of its
        // own. The app's `tmux-swarm` chair is launched as an agent instead. A child pane already
        // has an agent id, and registering it would take its pane from its own session.
        if env::var_os("SWARM_AGENT_ID").is_none()
            && matches!(adapter.as_str(), "herdr" | "superset" | "tmux")
            && own_pane().is_ok()
        {
            add_agent(
                &connection,
                &root,
                &adapter,
                &session,
                "orchestrator",
                "orchestrator",
            )?;
        }
        println!("{session}");
        return Ok(());
    }
    if let [cmd, sub, value] = args
        && cmd == "session"
        && sub == "chair"
    {
        let (session_id, agent_id) = identity()?;
        let chair = parse_chair(value)?;
        set_chair_for_caller(&connection, &session_id, &agent_id, chair)?;
        record_pi_log(&connection, &session_id, chair)?;
        return Ok(());
    }
    if let [cmd, sub, new_id, old_id] = args
        && cmd == "session"
        && sub == "continue"
    {
        return swarm::store::continue_session(&connection, new_id, old_id);
    }
    if let [cmd, sub, ids @ ..] = args
        && cmd == "session"
        && matches!(sub.as_str(), "archive" | "unarchive")
        && !ids.is_empty()
    {
        let ids = ids
            .iter()
            .map(|id| {
                let id = uuid::Uuid::parse_str(id).map_err(|_| USAGE)?;
                if id.get_version_num() != 7 {
                    return Err(USAGE);
                }
                Ok(id.to_string())
            })
            .collect::<Result<Vec<_>, &str>>()?;
        if sub == "archive" {
            swarm::store::archive_sessions(&mut connection, &ids)?;
        } else {
            swarm::store::unarchive_sessions(&mut connection, &ids)?;
        }
        return Ok(());
    }
    if let [cmd, json, flags @ ..] = args
        && cmd == "sessions"
        && json == "--json"
        && (flags.is_empty() || matches!(flags, [flag] if flag == "--archived"))
    {
        let mut sessions = Vec::new();
        for row in swarm::store::sessions_with_archived(&connection, !flags.is_empty())? {
            let chair_log = resolved_chair_log(&row);
            if let Some(path) = &chair_log {
                let path_text = path.to_string_lossy();
                if row.chair_log.as_deref() != Some(path_text.as_ref()) {
                    swarm::store::set_chair_log(&connection, &row.id, path)?;
                }
            }
            sessions.push(swarm::bus::Session {
                id: row.id,
                talk_mode: row.talk_mode,
                adapter: row.adapter,
                cwd: row.cwd,
                created_at: row.created_at,
                chair_provider: row.chair_provider,
                chair_id: row.chair_id,
                chair_log: chair_log.map(|path| path.to_string_lossy().into_owned()),
                continuation_of: row.continuation_of,
                agents: row.agents,
                messages: row.messages,
                last_message_at: row.last_message_at,
                archived_at_s: row.archived_at,
            });
        }
        return print_json(&swarm::bus::SessionList { sessions });
    }
    if let [cmd, sub, agent_id, role] = args
        && cmd == "agent"
        && sub == "add"
    {
        return add_agent(
            &connection,
            &root,
            &adapter_name(),
            &session_id()?,
            agent_id,
            role,
        );
    }
    if let [cmd, json] = args
        && cmd == "agents"
        && json == "--json"
    {
        let session_id = session_id()?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        return print_json(&list_agents(&mut connection, &root, &session_id, &adapter)?);
    }
    if let [cmd, json, all] = args
        && cmd == "agents"
        && json == "--json"
        && all == "--all"
    {
        // SwarmCLIBus gives CapturedProcess 20 s; reserve 2 s for startup, DB work, and JSON.
        let listings =
            all_agent_listings(&mut connection, &root, std::time::Duration::from_secs(18))?;
        return print_json(&listings);
    }
    if let [cmd, rest @ ..] = args
        && cmd == "messages"
    {
        let after = match rest {
            [json] if json == "--json" => -1,
            [json, flag, seq] if json == "--json" && flag == "--after" => {
                seq.parse().map_err(|_| format!("swarm: bad seq {seq}"))?
            }
            _ => return Err(USAGE.into()),
        };
        let messages = swarm::store::messages(&connection, &session_id()?, after)?
            .into_iter()
            .map(|row| swarm::bus::Message {
                seq: row.seq,
                sender: row.sender,
                recipient: row.recipient,
                kind: row.kind,
                body: std::fs::read_to_string(root.join(row.body_path)).ok(),
                created_at: row.created_at,
                read: row.read,
                delivery: row.delivery,
            })
            .collect();
        return print_json(&swarm::bus::MessageList { messages });
    }
    if let [cmd, agent_id, role, rest @ ..] = args
        && cmd == "launch"
    {
        if !swarm::bus::valid_agent_id(agent_id) {
            return Err(format!("swarm: bad agent id {agent_id}").into());
        }
        let herdr_agent_pane = env::var("HERDR_AGENT_PANE").ok();
        if let Some(reason) = swarm::bus::launch_refusal(
            env::var("SWARM_AGENT_ID").ok().as_deref(),
            herdr_agent_pane.as_deref(),
        ) {
            return Err(reason.into());
        }
        // Fail before any trust write, so a stray launch outside a session changes nothing.
        session_id()?;
        let (options, extra) = match rest.iter().position(|arg| arg == "--") {
            Some(index) => (&rest[..index], &rest[index + 1..]),
            None => (rest, &[][..]),
        };
        let (mut account, mut cwd, mut requested_provider, mut requested_model) =
            (Some("auto"), None, None, None);
        for pair in options.chunks(2) {
            match pair {
                [flag, value] if flag == "--account" => account = Some(value.as_str()),
                [flag, value] if flag == "--cwd" => cwd = Some(std::path::PathBuf::from(value)),
                [flag, value] if flag == "--provider" && Provider::parse(value).is_some() => {
                    requested_provider = Some(value.as_str())
                }
                [flag, value] if flag == "--model" => requested_model = Some(value.as_str()),
                _ => return Err(USAGE.into()),
            }
        }
        let cwd = cwd.map_or_else(env::current_dir, Ok)?;
        let cwd = std::fs::canonicalize(&cwd)
            .map_err(|error| format!("swarm: bad --cwd {}: {error}", cwd.display()))?;
        let (resolved, probe): (swarm::bus::ResolvedRole, Probe) = match requested_model {
            Some(_) if role != "chat" => {
                return Err("swarm: --model requires the chat role".into());
            }
            // A one-off chat pick runs exactly that runner, with no fallback (ADR 0033).
            Some(model) => {
                let provider = requested_provider
                    .and_then(Provider::parse)
                    .ok_or("swarm: a chat --model needs --provider")?;
                let (config, _) =
                    swarm::config::load().map_err(|error| format!("swarm: {error}"))?;
                let runner = swarm::config::one_off(&config.profiles[0], provider, model)
                    .map_err(|error| format!("swarm: {error}"))?;
                (
                    serde_json::from_value(serde_json::to_value(runner)?)?,
                    Probe::new(&config, account),
                )
            }
            None => {
                let (value, probe) = resolve_role(role, requested_provider, account, true)?;
                let resolved = serde_json::from_value(value)
                    .map_err(|error| format!("swarm: cannot resolve role {role}: {error}"))?;
                (resolved, probe)
            }
        };
        let account_deadline = probe.deadline;
        if let Some(reason) = swarm::bus::fable_refusal(agent_id, role, resolved.model.as_deref()) {
            return Err(reason.into());
        }
        let provider = resolved.provider.clone();
        let mut command =
            swarm::bus::argv(agent_id, role, &resolved, &swarm::paths::home()?, &cwd)?;
        let kind = Provider::parse(provider.as_deref().unwrap_or_default())
            .ok_or("swarm: launch has no known provider")?;
        let runner = swarm::config::Runner {
            provider: kind,
            model: resolved.model.clone().ok_or("swarm: launch has no model")?,
            effort: resolved
                .effort
                .clone()
                .ok_or("swarm: launch has no effort")?,
            sandbox: resolved.sandbox.clone(),
            approval: resolved.approval.clone(),
            permission: resolved.permission.clone(),
        };
        let mut extra = swarm::bus::extra_args(kind, extra)?;
        // Trust and pane environment must use the same native pick.
        // A provider with no account source launches on its CLI's own login, so `auto` means
        // nothing there; the chat profile passes it whichever runner starts.
        let picked = match (account, kind.has_accounts()) {
            (Some(requested), true) => {
                let mut cache = probe.accounts.borrow_mut();
                let accounts = cache
                    .entry(kind)
                    .or_insert_with(|| pick_accounts(kind, account_deadline, requested == "auto"));
                select_account(
                    kind,
                    requested,
                    resolved.model.as_deref(),
                    probe.min_usage_left_pct,
                    accounts.as_ref().map_err(Clone::clone),
                )?
            }
            _ => None,
        };
        let mut pane_dir = cwd.clone();
        let user_home = std::path::PathBuf::from(env_var("HOME")?);
        // The app makes a chair launch in the folder the owner picked, and hides its pane, so
        // that pick is consent for that one folder (owner answer I1, ADR 0043).
        let consent = if standing_consent() {
            TrustConsent::Standing
        } else if agent_id == "orchestrator" {
            TrustConsent::Picked
        } else {
            TrustConsent::Ask
        };
        let launch_trust = LaunchTrust {
            store: &connection,
            consent,
            // Auto keeps the base rule for a launch with no account: a file that cannot take the
            // entry is a warning. Only a named account must get it.
            required: picked.is_some() && account.is_some_and(|name| name != "auto"),
            cwd: &cwd,
        };
        // Codex has no per-launch hook to name its chair, and the app lists and reads a chat only
        // by its chair provider. Its thread id comes later with its log.
        if kind == Provider::Codex && agent_id == "orchestrator" {
            swarm::store::set_chair_provider(&connection, &session_id()?, "codex")?;
        }
        match kind {
            Provider::Codex | Provider::Agy => match trust_target(&cwd, &user_home) {
                Ok(target) if kind == Provider::Codex => {
                    // A selected account needs trust in its own Codex home.
                    let homes = if let Some(account) = &picked {
                        vec![std::path::PathBuf::from(&account.home)]
                    } else {
                        codex_homes(&user_home)?
                    };
                    launch_trust.run("codex", &target, &homes, swarm::bus::codex_trust_plan)?;
                }
                Ok(target) => {
                    // AGY has no accounts, and its one settings file must take the entry.
                    let agy = LaunchTrust {
                        required: true,
                        ..launch_trust
                    };
                    let settings = vec![agy_settings(&user_home)];
                    agy.run("agy", &target, &settings, swarm::bus::agy_trust_plan)?;
                }
                Err(reason) => eprintln!(
                    "swarm: not pre-trusting for {}: {reason}; answer the prompt in the pane",
                    kind.id()
                ),
            },
            Provider::Pi => {
                // pi has no hooks. It takes the session id swarm gives it, so its log is found by
                // that id, and it reads the host contract from its system prompt (ADR 0086).
                let session = session_id()?;
                let id = format!("{session}-{agent_id}");
                let mut args = vec!["--session-id".to_string(), id.clone()];
                if let Some(context) = pane_context(kind.id(), &session, agent_id) {
                    args.extend(["--append-system-prompt".into(), context]);
                }
                extra.splice(0..0, args);
                if agent_id == "orchestrator" {
                    swarm::store::set_chair(&connection, &session, Some((kind.id(), &id)))?;
                }
            }
            Provider::Claude => {
                // The app hides the chair's pane, so a trust dialog there waits for nobody. The
                // chair runs in cwd itself, as a resuming child does (ADR 0008).
                if agent_id != "orchestrator" {
                    let (dir, args) =
                        swarm::bus::claude_child(agent_id, &cwd, &extra, &uuid::Uuid::now_v7());
                    // A checked-out repo can commit `.herdr` or `.herdr/workers` as a link, and
                    // trusting where it points could trust any folder, such as `/`. The check
                    // stops create_dir_all from making dirs through a committed link. A resuming
                    // child runs in cwd itself, so nothing below cwd is checked for it.
                    for path in dir.ancestors().take_while(|path| *path != cwd) {
                        if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()) {
                            return Err(format!(
                                "swarm: {} is a symlink; refusing to trust where it points",
                                path.display()
                            )
                            .into());
                        }
                    }
                    std::fs::create_dir_all(&dir)?;
                    pane_dir = std::fs::canonicalize(&dir)?;
                    // cwd is canonical, so a link made after the check still shows here.
                    if pane_dir != dir {
                        return Err(format!(
                            "swarm: {} resolves to {}; refusing to trust it",
                            dir.display(),
                            pane_dir.display()
                        )
                        .into());
                    }
                    extra = args;
                }
                // The app lets the owner pick any folder for a chair, such as $HOME or a shared
                // one, and consent covers only a folder that passes the check (ADR 0043), so a
                // chair or a seat gets the entry only where Codex and AGY may be pre-trusted.
                if let Err(reason) = trust_target(&cwd, &user_home) {
                    eprintln!(
                        "swarm: not pre-trusting for claude: {reason}; answer the prompt in the pane"
                    );
                } else {
                    let configs = vec![user_home.join(".claude.json")];
                    launch_trust.run(
                        "claude",
                        &pane_dir,
                        &configs,
                        swarm::bus::claude_trust_plan,
                    )?;
                }
            }
        }
        command.extend(extra);
        // Every adapter's spawn verb opens the pane in the working directory it runs in.
        env::set_current_dir(&pane_dir)?;
        return spawn_agent(
            &connection,
            &root,
            agent_id,
            role,
            SpawnOptions {
                provider: provider.as_deref(),
                account: picked.as_ref().map(|picked| picked.name.as_str()),
                resolved_account: picked.as_ref(),
                command: &command,
                runner: Some(&runner),
            },
            account_deadline,
        );
    }
    if let [cmd, agent_id, role, rest @ ..] = args
        && cmd == "spawn"
    {
        if let Some(reason) = swarm::bus::launch_refusal(
            env::var("SWARM_AGENT_ID").ok().as_deref(),
            env::var("HERDR_AGENT_PANE").ok().as_deref(),
        ) {
            return Err(reason.into());
        }
        let options = parse_spawn_options(rest)?;
        return spawn_agent(
            &connection,
            &root,
            agent_id,
            role,
            options,
            swarm::profiles::native::deadline(ACCOUNT_PICK_TIMEOUT_SECONDS),
        );
    }
    if let [cmd] = args
        && cmd == "drain"
    {
        let summarizer = env_var("SWARM_SUMMARIZER")?;
        while let Some(job_id) = swarm::store::claim_next(&connection)? {
            println!(
                "{}",
                process_summary_job(&mut connection, &root, &adapter_name(), &summarizer, job_id)?
            );
        }
        return Ok(());
    }
    if let [cmd, child] = args
        && cmd == "close"
    {
        let session_id = session_id()?;
        let pane = swarm::store::require_pane(&connection, &session_id, child)?;
        swarm::adapter::load(&root, &adapter_name())?.run("close", &[("pane", &pane)])?;
        return swarm::store::clear_pane(&connection, &session_id, child);
    }
    if let [cmd, agent_id, json] = args
        && cmd == "commands"
        && json == "--json"
    {
        let session_id = session_id()?;
        let (cwd, chair_provider): (String, Option<String>) = connection.query_row(
            "SELECT cwd, chair_provider FROM session WHERE id = ?1",
            [&session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let agent = swarm::store::agents(&connection, &session_id)?
            .into_iter()
            .find(|agent| agent.id == *agent_id)
            .ok_or("swarm: unknown agent")?;
        let provider = agent
            .provider
            .or(chair_provider.filter(|_| agent_id == "orchestrator"))
            .as_deref()
            .and_then(swarm::providers::Provider::parse)
            .ok_or("swarm: this agent's CLI is unknown")?;
        let env = match agent.account.as_deref() {
            Some(name) if provider.has_accounts() => {
                swarm::profiles::native::launch_env(provider, name)?
            }
            _ => Default::default(),
        };
        // ponytail: the store keeps no folder per agent, so a child launched with another
        // --cwd gets the chat folder's list. Store the launch folder if that shows up wrong.
        let deadline = swarm::profiles::native::deadline(15);
        return print_json(&swarm::commands::list(
            provider,
            &env,
            std::path::Path::new(&cwd),
            deadline,
        ));
    }
    if let [cmd, agent_id] = args
        && cmd == "type"
    {
        let text = std::io::read_to_string(std::io::stdin())?;
        if text.trim().is_empty() {
            return Err("swarm: empty text".into());
        }
        let session_id = session_id()?;
        let pane = swarm::store::require_pane(&connection, &session_id, agent_id)?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        if !adapter.has_pane(&pane)? {
            return Err(
                "swarm: this chat's pane has closed; start a new chat or switch model".into(),
            );
        }
        // Marked before typing, which takes a second or more, so the row turns at once.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;
        swarm::store::mark_sent(&connection, &session_id, agent_id, Some(now))?;
        let typed = type_text(&connection, &adapter, &session_id, agent_id, &pane, &text);
        if typed.is_err() {
            swarm::store::mark_sent(&connection, &session_id, agent_id, None)?;
        }
        return typed;
    }
    if let [cmd, agent_id, prompt_id, choice] = args
        && cmd == "answer"
    {
        // Only the owner answers, from the app, so a model cannot answer a question by mistake.
        if in_agent_pane() {
            return Err(
                "swarm: only the owner answers an agent's question; the Swarm app sends it".into(),
            );
        }
        let choice: usize = choice
            .parse()
            .map_err(|_| format!("swarm: choice {choice:?} is not a number"))?;
        let pane = swarm::store::require_pane(&connection, &session_id()?, agent_id)?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        // One answer per pane at a time, so the chair's card and the column's card cannot mix
        // their keys. The lock ends with this process.
        let lock =
            std::fs::File::create(root.join(format!("answer-{}.lock", pane.replace('/', "_"))))?;
        lock.try_lock()
            .map_err(|_| format!("swarm: an answer to {agent_id} is still being sent"))?;
        let read = || {
            let pane = [("pane", pane.as_str())];
            let screen = adapter.screen(&pane, std::time::Duration::from_secs(1))?;
            swarm::screen::whole_prompt(&screen, || {
                adapter.capture_within(&pane, std::time::Duration::from_secs(1))
            })
        };
        let prompt = read().ok_or(format!("swarm: {agent_id} shows no question now"))?;
        if prompt.id != *prompt_id {
            return Err("swarm: the question changed; read it again".into());
        }
        let keys = prompt
            .keys(choice)
            .ok_or(format!("swarm: the question has no choice {choice}"))?;
        // Each key after the first goes only once the same question shows the cursor where the
        // keys before it put it.
        let settled = |cursor: usize| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            loop {
                if read().is_some_and(|now| now.id == prompt.id && now.cursor == cursor) {
                    return true;
                }
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        };
        let mut cursor = prompt.cursor;
        for (step, key) in keys.iter().enumerate() {
            if step > 0 && !settled(cursor) {
                return Err(format!(
                    "swarm: {agent_id}'s question changed while the answer was sent; read it again"
                )
                .into());
            }
            adapter.run("key", &[("pane", &pane), ("key", key)])?;
            cursor = match key.as_str() {
                "Down" => cursor + 1,
                "Up" => cursor.saturating_sub(1),
                "Enter" => cursor,
                _ => choice,
            };
        }
        return Ok(());
    }
    if let [cmd, agent_id] = args
        && cmd == "interrupt"
    {
        let pane = swarm::store::require_pane(&connection, &session_id()?, agent_id)?;
        swarm::adapter::load(&root, &adapter_name())?.run("interrupt", &[("pane", &pane)])?;
        return Ok(());
    }
    if let [cmd, agent_id, key] = args
        && cmd == "key"
    {
        // The app's pull-back of a queued message. Escape and C-c would stop the agent's turn.
        if in_agent_pane() {
            return Err(
                "swarm: only the owner presses keys in an agent's pane; the Swarm app sends them"
                    .into(),
            );
        }
        if !["Up", "C-u"].contains(&key.as_str()) {
            return Err(format!("swarm: key {key:?} is not allowed; only Up and C-u").into());
        }
        let pane = swarm::store::require_pane(&connection, &session_id()?, agent_id)?;
        swarm::adapter::load(&root, &adapter_name())?
            .run("key", &[("pane", &pane), ("key", key)])?;
        return Ok(());
    }
    let (session_id, agent_id) = identity()?;
    match args {
        [cmd, recipient, kind] if cmd == "send" => {
            let body = std::io::read_to_string(std::io::stdin())?;
            let seq = deliver(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                recipient,
                kind,
                &body,
            )?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "finish" => {
            let summary = std::io::read_to_string(std::io::stdin())?;
            let orchestrator = swarm::store::orchestrator_of(&connection, &session_id)?;
            let seq = deliver(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                &orchestrator,
                "summary",
                &summary,
            )?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "exited" => {
            let pane = swarm::store::require_pane(&connection, &session_id, &agent_id)?;
            let text =
                swarm::adapter::load(&root, &adapter_name())?.run("capture", &[("pane", &pane)])?;
            let run_dir = root.join(format!("runs/{session_id}"));
            std::fs::create_dir_all(&run_dir)?;
            swarm::store::write_atomic(&run_dir.join(format!("{agent_id}.log")), &text)?;
            let note = format!("agent {agent_id} exited without a summary");
            report_dead(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                &note,
            )
        }
        [cmd, rest @ ..] if cmd == "sweep" => {
            let every = match rest {
                [] => None,
                [flag, secs] if flag == "--every" => {
                    Some(secs.parse::<u64>().ok().filter(|s| *s > 0).ok_or(USAGE)?)
                }
                _ => return Err(USAGE.into()),
            };
            if let Some(refusal) = swarm::bus::sweep_refusal(&adapter_name(), every.is_some()) {
                return Err(refusal.into());
            }
            let adapter = swarm::adapter::load(&root, &adapter_name())?;
            loop {
                let mut lines = Vec::new();
                let pass = sweep_once(
                    &mut connection,
                    &root,
                    &adapter,
                    &session_id,
                    &agent_id,
                    &mut lines,
                );
                lines.iter().for_each(|line| println!("{line}"));
                match pass {
                    Ok(()) => {}
                    Err(error) if every.is_some() => eprintln!("swarm: sweep skipped: {error}"),
                    Err(error) => return Err(error),
                }
                let Some(secs) = every else { return Ok(()) };
                std::thread::sleep(std::time::Duration::from_secs(secs));
            }
        }
        [cmd] if cmd == "inbox" => {
            for m in swarm::store::inbox(&connection, &session_id, &agent_id)? {
                println!("{} {} {} {}", m.seq, m.sender_id, m.kind, m.body_path);
            }
            Ok(())
        }
        [cmd, seq] if cmd == "ack" => {
            let seq: i64 = seq.parse().map_err(|_| format!("swarm: bad seq {seq}"))?;
            ack(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                seq,
            )
        }
        _ => Err(USAGE.into()),
    }
}

/// `swarm guard <provider> <event>`: the rule list's verdict on one tool call, in the CLI's own
/// reply format (ADR 0040). It never opens the store, so a busy bus cannot slow a tool call.
fn guard(args: &[String]) -> (String, String, i32) {
    let [provider, event] = args else {
        return (String::new(), format!("{USAGE}\n"), 2);
    };
    let Some(provider) = Provider::parse(provider) else {
        return (String::new(), format!("{USAGE}\n"), 2);
    };
    let deadline = std::time::Instant::now() + swarm::guard::DEADLINE;
    let verdict = match (swarm::paths::guards_file(), env::var("HOME")) {
        (Ok(path), Ok(home)) => {
            let path = path.to_string_lossy();
            let payload = read_within(std::io::stdin(), std::time::Duration::from_secs(2));
            match (std::fs::read_to_string(path.as_ref()), payload) {
                (_, None) => swarm::guard::Verdict::Deny(
                    "swarm guard: cannot read the hook payload on stdin within 2 s, so the call is blocked".into(),
                ),
                (Err(error), _) if error.kind() != std::io::ErrorKind::NotFound => {
                    swarm::guard::Verdict::Deny(format!(
                        "swarm guard: cannot read {path}: {error}, so the call is blocked"
                    ))
                }
                (list, Some(payload)) => swarm::guard::decide(
                    list.ok().as_deref(),
                    &path,
                    provider,
                    event,
                    &payload,
                    &home,
                    deadline,
                ),
            }
        }
        _ => swarm::guard::Verdict::Deny(
            "swarm guard: HOME is not set, so the call is blocked".into(),
        ),
    };
    swarm::guard::render(provider, &verdict)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let [cmd, rest @ ..] = args.as_slice()
        && cmd == "guard"
    {
        // A panic exits 101, which Claude and Codex read as "let the call through"; exit 2 blocks
        // on every CLI.
        std::panic::set_hook(Box::new(|info| {
            eprintln!("swarm guard: {info}, so the call is blocked");
            std::process::exit(2);
        }));
        let (stdout, stderr, code) = guard(rest);
        print!("{stdout}");
        eprint!("{stderr}");
        std::process::exit(code);
    }
    if let [cmd] = args.as_slice()
        && cmd == "status-feed"
    {
        if let Err(error) = status_feed() {
            eprintln!("swarm status-feed: {error}");
        }
        return;
    }
    if let [cmd, rest @ ..] = args.as_slice()
        && cmd == "hook"
    {
        if let Err(error) = hook(rest) {
            eprintln!("{error}");
        }
        println!("{{}}");
        return;
    }
    if let [cmd, agent_id] = args.as_slice()
        && cmd == "attach"
    {
        match attach(agent_id) {
            Ok(status) => std::process::exit(
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)),
            ),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if let Err(error) = run(&args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_menu_lists_each_account_and_reads_a_number_a_name_or_enter() {
        let mut list = swarm::profiles::empty_accounts("claude");
        for (name, pct, auth) in [
            ("work", Some(87), swarm::profiles::AuthState::SignedIn),
            ("personal", None, swarm::profiles::AuthState::SignedOut),
        ] {
            list.accounts.push(swarm::profiles::Account {
                name: name.into(),
                email: Some(format!("{name}@example.com")),
                home: String::new(),
                env: Default::default(),
                auth_state: auth,
                usage_state: String::new(),
                usage_source: None,
                remaining_pct: pct,
                summary: None,
                invalid_home: false,
                fable_spent: false,
                renewable: false,
            });
        }
        let pick = |list: &swarm::profiles::AccountList, typed: &str| {
            let mut out = Vec::new();
            let choice = menu_choice(list, &mut typed.as_bytes(), &mut out);
            (choice, String::from_utf8(out).unwrap())
        };
        let (choice, shown) = pick(&list, "2\n");
        assert_eq!(choice.unwrap(), "personal");
        assert!(shown.contains(" 1) work      work@example.com"), "{shown}");
        assert!(
            shown.contains("87% left") && shown.contains("signed out"),
            "{shown}"
        );
        assert_eq!(pick(&list, "work\n").0.unwrap(), "work");
        assert_eq!(pick(&list, "\n").0.unwrap(), "auto");
        assert!(pick(&list, "3\n").0.is_err());
        list.accounts.clear();
        assert!(pick(&list, "1\n").0.unwrap_err().contains("accounts login"));
    }

    #[test]
    fn login_adapter_returns_before_the_swift_process_limit() {
        let deadline = swarm::profiles::native::deadline(LOGIN_ADAPTER_TIMEOUT_SECONDS);
        assert!(
            deadline.duration_since(std::time::Instant::now())
                <= std::time::Duration::from_secs(18)
        );
    }

    #[test]
    fn rering_finishes_after_the_session_deadline() {
        let root =
            std::env::temp_dir().join(format!("swarm-ring-deadline-{}", uuid::Uuid::now_v7()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "chair").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
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
        connection
            .execute("UPDATE message SET created_at = unixepoch() - 61", [])
            .unwrap();
        let mut adapter = swarm::adapter::parse("fake", "self = true\nspawn = true\nring = printf text >> \"$SWARM_PANE\"; sleep 0.3; printf enter >> \"$SWARM_PANE\"\nlist = true\nclose = true\ncapture = true\n").unwrap();
        adapter.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
        let ring_log = root.join("ring");
        rering_if_due(
            &mut connection,
            &root,
            &adapter,
            &session,
            CODER,
            ring_log.to_str().unwrap(),
            Proof::Wait,
        )
        .unwrap();
        let rings: i64 = connection
            .query_row("SELECT rings FROM message", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rings, 1);
        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), "textenter");
        assert!(adapter.check_deadline().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_hung_ring_ends_at_the_ring_timeout() {
        let (root, mut connection, session) =
            ring_session("hung-ring", Some("codex"), "ring = sleep 60", "› \n");
        let started = std::time::Instant::now();
        let seq = send_task(&root, &mut connection, &session);
        assert!(started.elapsed() < RING_TIMEOUT + std::time::Duration::from_secs(2));
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_rering_presses_enter_after_the_session_deadline() {
        let root = std::env::temp_dir().join(format!("swarm-claude-ring-{}", uuid::Uuid::now_v7()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "chair").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
        swarm::store::set_provider(&connection, &session, CODER, "claude").unwrap();
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
        connection
            .execute("UPDATE message SET created_at = unixepoch() - 61", [])
            .unwrap();
        let screen = root.join("screen");
        let log = root.join("log");
        std::fs::write(&screen, "────\n❯ \n────\n").unwrap();
        // The ring stays typed in the composer until a key clears it.
        let mut adapter = swarm::adapter::parse(
            "fake",
            &format!(
                "self = true\nspawn = true\nlist = true\nclose = true\n\
                 capture = cat \"$SWARM_PANE\"\n\
                 ring = echo ring >> '{log}'; printf '────\\n❯ %s\\n────\\n' \"$SWARM_TEXT\" > \"$SWARM_PANE\"\n\
                 key = echo \"$SWARM_KEY\" >> '{log}'; printf '────\\n❯ \\n────\\n' > \"$SWARM_PANE\"\n",
                log = log.display()
            ),
        )
        .unwrap();
        adapter.deadline = Some(std::time::Instant::now());
        rering_if_due(
            &mut connection,
            &root,
            &adapter,
            &session,
            CODER,
            screen.to_str().unwrap(),
            Proof::Wait,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "ring\nEnter\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_keeps_a_listing_after_a_late_rering() {
        let root =
            std::env::temp_dir().join(format!("swarm-listing-rering-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        let ring_log = root.join("ring");
        let ring_path = swarm::adapter::shell_line(&[ring_log.to_string_lossy().into_owned()]);
        std::fs::write(root.join("adapters/fake.conf"),
            format!("self = true\nspawn = true\nring = sleep 1.2; printf rung > {ring_path}\nlist = printf pane\nclose = true\ncapture = true\nscreen = printf '{{\"result\":{{\"agent\":{{\"agent_status\":\"idle\"}}}}}}'\n")
        ).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let session =
            swarm::store::create_session(&connection, "lane", &root, None, Some("fake")).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "chair").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "pane").unwrap();
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
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - ?1",
                [RERING_UNSEEN_AFTER_SECS + 1],
            )
            .unwrap();
        let started = std::time::Instant::now();
        let listings =
            all_agent_listings(&mut connection, &root, std::time::Duration::from_secs(1)).unwrap();
        assert!(started.elapsed() > std::time::Duration::from_secs(1));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "rung");
        assert_eq!(listings[&session].agents.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_omits_slow_sessions_and_keeps_fast_sessions_within_budget() {
        let root =
            std::env::temp_dir().join(format!("swarm-batch-deadline-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let mut ids = Vec::new();
        for (index, (name, list, screen)) in [
            ("slow-list", "sleep 3; printf pane", "true"),
            ("slow-screen", "sleep 0.4; printf pane", "sleep 3"),
            ("fast", "printf pane", "true"),
        ]
        .into_iter()
        .enumerate()
        {
            std::fs::write(root.join("adapters").join(format!("{name}.conf")), format!(
                "self = true\nspawn = true\nring = true\nlist = {list}\nclose = true\ncapture = true\nscreen = {screen}\n"
            )).unwrap();
            let session =
                swarm::store::create_session(&connection, "lane", &root, None, Some(name)).unwrap();
            swarm::store::add_agent(&connection, &session, "coder", "code").unwrap();
            swarm::store::set_pane(&connection, &session, "coder", "pane").unwrap();
            // The hung sessions run first, so they cannot starve the healthy session behind them.
            connection
                .execute(
                    "UPDATE session SET created_at = ?1 WHERE id = ?2",
                    rusqlite::params![3 - index as i64, session],
                )
                .unwrap();
            ids.push(session);
        }
        let started = std::time::Instant::now();
        let listings = all_agent_listings(
            &mut connection,
            &root,
            std::time::Duration::from_millis(1500),
        )
        .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert!(!listings.contains_key(&ids[0]));
        assert!(!listings.contains_key(&ids[1]));
        assert_eq!(listings[&ids[2]].agents.len(), 1);
        // The single-session list keeps its original unbounded list and bounded screen behavior.
        let adapter = swarm::adapter::load(&root, "slow-list").unwrap();
        assert!(list_agents(&mut connection, &root, &ids[0], &adapter).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_lists_the_panes_of_one_adapter_once() {
        let root =
            std::env::temp_dir().join(format!("swarm-batch-list-once-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        let calls = root.join("calls");
        let calls_path = swarm::adapter::shell_line(&[calls.to_string_lossy().into_owned()]);
        std::fs::write(root.join("adapters/fake.conf"), format!(
            "self = true\nspawn = true\nring = true\nlist = echo >> {calls_path}; printf pane\nclose = true\ncapture = true\nscreen = true\n"
        )).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let session =
                swarm::store::create_session(&connection, "lane", &root, None, Some("fake"))
                    .unwrap();
            swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
            swarm::store::set_pane(&connection, &session, CODER, "pane").unwrap();
            ids.push(session);
        }
        let listings =
            all_agent_listings(&mut connection, &root, std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(std::fs::read_to_string(&calls).unwrap().lines().count(), 1);
        for id in &ids {
            assert_eq!(listings[id].agents.len(), 1);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_gives_its_budget_to_sessions_with_a_live_pane() {
        let root =
            std::env::temp_dir().join(format!("swarm-batch-live-share-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        std::fs::write(root.join("adapters/fake.conf"),
            "self = true\nspawn = true\nring = true\nlist = printf pane\nclose = true\ncapture = true\nscreen = sleep 0.2; printf '{\"result\":{\"agent\":{\"agent_status\":\"idle\"}}}'\n"
        ).unwrap();
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let mut live = String::new();
        // The live session lists first; an even split would give it 1/21 of the budget.
        for index in 0..21 {
            let session =
                swarm::store::create_session(&connection, "lane", &root, None, Some("fake"))
                    .unwrap();
            swarm::store::add_agent(&connection, &session, CODER, "code").unwrap();
            let pane = if index == 0 { "pane" } else { "closed" };
            swarm::store::set_pane(&connection, &session, CODER, pane).unwrap();
            connection
                .execute(
                    "UPDATE session SET created_at = ?1 WHERE id = ?2",
                    rusqlite::params![100 - index as i64, session],
                )
                .unwrap();
            if index == 0 {
                live = session;
            }
        }
        let listings =
            all_agent_listings(&mut connection, &root, std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(listings.len(), 21);
        assert!(listings[&live].agents[0].alive == Some(true));
        std::fs::remove_dir_all(root).unwrap();
    }

    const ORCHESTRATOR: &str = "orchestrator";
    const CODER: &str = "coder";

    #[test]
    fn a_run_script_refuses_an_agent_id_that_leaves_runs() {
        let root =
            std::env::temp_dir().join(format!("swarm-script-escape-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let session = "0199a000-0000-7000-8000-000000000001";
        for bad in ["../../x", "../x", "a/b", "X", ""] {
            assert!(script_line(&root, session, bad, "true").is_err(), "{bad}");
        }
        assert!(script_line(&root, "../session", "coder", "true").is_err());
        assert!(!root.join("x.sh").exists() && !root.parent().unwrap().join("x.sh").exists());
        assert!(!root.join("runs").exists());
        let line = script_line(&root, session, "coder", "true").unwrap();
        assert!(
            line.ends_with(&format!("runs/{session}/coder.sh'")),
            "{line}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_archived_codex_rollout_is_found_under_archived_sessions() {
        let codex_home =
            std::env::temp_dir().join(format!("swarm-archived-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&codex_home);
        let name = "rollout-2026-09-30T10-00-00-0199a000-0000-7000-8000-000000000001.jsonl";
        let recorded = codex_home.join("sessions/2026/09/30").join(name);
        let archived = codex_home.join("archived_sessions").join(name);
        std::fs::create_dir_all(archived.parent().unwrap()).unwrap();
        std::fs::write(&archived, "{}\n").unwrap();
        let recorded = recorded.to_string_lossy().into_owned();
        assert_eq!(
            resolved_agent_log(recorded.clone()),
            archived.to_string_lossy()
        );

        // A log still in place, or one that is nowhere, stays as recorded.
        std::fs::create_dir_all(codex_home.join("sessions/2026/09/30")).unwrap();
        std::fs::write(&recorded, "{}\n").unwrap();
        assert_eq!(resolved_agent_log(recorded.clone()), recorded);
        let gone = codex_home.join("sessions/2026/09/30/rollout-gone.jsonl");
        let gone = gone.to_string_lossy().into_owned();
        assert_eq!(resolved_agent_log(gone.clone()), gone);
        let _ = std::fs::remove_dir_all(&codex_home);
    }

    #[test]
    fn hook_stdin_read_ends_at_its_deadline_while_the_writer_holds_the_pipe() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut writer, b"{}").unwrap();
        let started = std::time::Instant::now();
        assert_eq!(
            read_within(reader, std::time::Duration::from_millis(200)),
            None
        );
        assert!(started.elapsed() < std::time::Duration::from_millis(600));
        drop(writer);
        let (reader, mut writer) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut writer, b"{}").unwrap();
        drop(writer);
        assert_eq!(
            read_within(reader, std::time::Duration::from_millis(200)).as_deref(),
            Some("{}")
        );
    }

    #[test]
    fn a_broken_spare_codex_home_is_skipped_unless_its_account_was_picked() {
        let root = std::env::temp_dir().join(format!("swarm-spare-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (main, spare) = (root.join(".codex"), root.join(".codex-spare"));
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&spare).unwrap();
        std::fs::write(spare.join("config.toml"), "not toml = =\n").unwrap();
        let project = std::path::Path::new("/project");

        let store = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let codex = |home: &std::path::Path| {
            swarm::bus::write_trust(&store, || swarm::bus::codex_trust_plan(home, project))
                .map(|_| ())
        };
        trust_each(&[spare.clone(), main.clone()], false, codex).unwrap();
        assert!(
            std::fs::read_to_string(main.join("config.toml"))
                .unwrap()
                .contains("/project")
        );
        assert_eq!(
            std::fs::read_to_string(spare.join("config.toml")).unwrap(),
            "not toml = =\n"
        );
        assert!(trust_each(std::slice::from_ref(&spare), true, codex).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_read_only_spare_claude_profile_is_skipped_unless_its_account_was_picked() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-spare-claude-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (main, spare) = (root.join("main.json"), root.join("spare.json"));
        std::fs::write(&spare, "{}\n").unwrap();
        std::fs::set_permissions(&spare, std::fs::Permissions::from_mode(0o444)).unwrap();
        let project = std::path::Path::new("/project");
        let store = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let claude = |config: &std::path::Path| {
            swarm::bus::write_trust(&store, || swarm::bus::claude_trust_plan(config, project))
                .map(|_| ())
        };

        trust_each(&[spare.clone(), main.clone()], false, claude).unwrap();
        assert!(std::fs::read_to_string(&main).unwrap().contains("/project"));
        assert_eq!(std::fs::read_to_string(&spare).unwrap(), "{}\n");
        assert!(trust_each(std::slice::from_ref(&spare), true, claude).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_hook_outside_a_swarm_agent_reports_nothing() {
        let claude = ["claude".to_string()];
        let waiting = r#"{"hook_event_name":"PermissionRequest"}"#;
        let session = "0199a000-0000-7000-8000-000000000001";
        let no_env = |_: &str| None;
        assert!(hook_report(&claude, waiting, no_env).unwrap().is_none());
        let session_only = |name: &str| (name == "SWARM_SESSION_ID").then(|| session.to_string());
        assert!(
            hook_report(&claude, waiting, session_only)
                .unwrap()
                .is_none()
        );
        assert!(hook_report(&claude, "not json", no_env).unwrap().is_none());

        let coder = |name: &str| match name {
            "SWARM_SESSION_ID" => Some(session.to_string()),
            "SWARM_AGENT_ID" => Some(CODER.to_string()),
            _ => None,
        };
        let report = |state, log| {
            Some(HookReport {
                session: session.to_string(),
                agent: CODER.to_string(),
                state,
                log,
                usage_provider: None,
            })
        };
        assert_eq!(
            hook_report(&claude, waiting, coder).unwrap(),
            report(
                Some(swarm::host::HookState {
                    state: "waiting",
                    detail: None,
                    kind: Some("permission"),
                }),
                None
            )
        );
        assert!(
            hook_report(&claude, r#"{"hook_event_name":"SessionStart"}"#, coder)
                .unwrap()
                .is_none()
        );
        assert!(hook_report(&claude, "not json", coder).is_err());
        let agy_stop = ["agy".to_string(), "Stop".to_string()];
        assert_eq!(
            hook_report(&agy_stop, r#"{"error":"quota"}"#, coder).unwrap(),
            report(
                Some(swarm::host::HookState {
                    state: "failed",
                    detail: Some("quota".to_string()),
                    kind: None,
                }),
                None
            )
        );
        assert!(hook_report(&["gemini".to_string()], waiting, coder).is_err());
    }

    #[test]
    fn a_hook_reports_the_chat_log_its_payload_names() {
        let dir = std::env::temp_dir().join(format!("swarm-hook-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("chat.jsonl");
        std::fs::write(&log, "").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        let session = "0199a000-0000-7000-8000-000000000001";
        let coder = |name: &str| match name {
            "SWARM_SESSION_ID" => Some(session.to_string()),
            "SWARM_AGENT_ID" => Some(CODER.to_string()),
            _ => None,
        };
        let path = |value: &std::path::Path| serde_json::to_string(value).unwrap();
        let reported = |args: &[&str], payload: String| {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            hook_report(&args, &payload, coder).unwrap()
        };

        // A start event says nothing about state, yet its log path is kept.
        let start = format!(
            r#"{{"hook_event_name":"SessionStart","transcript_path":{}}}"#,
            path(&log)
        );
        let claude = reported(&["claude"], start).unwrap();
        assert_eq!((claude.state, claude.log), (None, Some(log.clone())));
        let codex = format!(
            r#"{{"hook_event_name":"PermissionRequest","transcript_path":{}}}"#,
            path(&log)
        );
        let codex = reported(&["codex"], codex).unwrap();
        assert_eq!(
            (codex.state, codex.log),
            (
                Some(swarm::host::HookState {
                    state: "waiting",
                    detail: None,
                    kind: Some("permission"),
                }),
                Some(log.clone())
            )
        );
        let agy = reported(
            &["agy", "PreInvocation"],
            format!(r#"{{"transcriptPath":{}}}"#, path(&log)),
        )
        .unwrap();
        assert_eq!(agy.log, Some(log.clone()));

        // Codex may send null; a relative, missing, or non-jsonl path is not a chat log.
        for bad in [
            "null".to_string(),
            r#""chat.jsonl""#.to_string(),
            path(&dir.join("missing.jsonl")),
            path(&dir.join("notes.txt")),
            path(&dir),
        ] {
            let payload =
                format!(r#"{{"hook_event_name":"SessionStart","transcript_path":{bad}}}"#);
            assert!(reported(&["claude"], payload).is_none(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        assert!(swarm::store::has_summary(&connection, &session, CODER).unwrap());
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "%1\n");
    }

    /// A fake Claude pane that draws its input box on the third read, keeps a ring unsent in the
    /// box as a starting Claude does, and starts a turn on Enter.
    #[test]
    fn a_ring_to_claude_waits_for_the_input_box_and_presses_enter_until_it_sends() {
        let (dir, connection, session) =
            ring_session("ring-pane", Some("claude"), "", "$ claude\n");
        let box_rows = |text: &str| format!("────\n❯ {text}\n────\n");
        std::fs::write(dir.join("empty"), box_rows("")).unwrap();
        std::fs::write(
            dir.join("working"),
            format!("✻ Thinking…\n{}", box_rows("")),
        )
        .unwrap();
        let d = dir.display();
        let adapter = swarm::adapter::parse(
            "fake",
            &format!(
                "self = true\nspawn = true\nlist = true\nclose = true\n\
                 capture = n=$(($(cat '{d}/reads' 2>/dev/null || echo 0) + 1)); echo $n > '{d}/reads'; \
                 [ $n = 3 ] && cp '{d}/empty' '{d}/screen'; echo read >> '{d}/log'; cat '{d}/screen'\n\
                 ring = echo ring >> '{d}/log'; printf '────\\n❯ %s\\n────\\n' \"$SWARM_TEXT\" > '{d}/screen'\n\
                 key = echo \"$SWARM_KEY\" >> '{d}/log'; cp '{d}/working' '{d}/screen'\n"
            ),
        )
        .unwrap();
        let delivery = ring_pane(
            &connection,
            &adapter,
            &dir,
            &session,
            CODER,
            "%2",
            Proof::Wait,
        )
        .unwrap();
        assert_eq!(delivery, Some(Delivery::Screen));
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "read\nread\nread\nring\nread\nEnter\nread\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A fake pi pane that draws its input box on the third read, keeps the text unsent in the
    /// box as a starting pi does, and starts a turn on Enter.
    #[test]
    fn typed_text_waits_for_the_input_box_and_presses_enter_until_it_sends() {
        let (dir, connection, session) = ring_session("type-pane", Some("pi"), "", "$ pi\n");
        let box_rows = |text: &str| format!("────\n{text}\n────\n~/project\n");
        std::fs::write(dir.join("empty"), box_rows("")).unwrap();
        std::fs::write(
            dir.join("working"),
            "hi\n── ⠏ Working ──\n\n────\n~/project\n",
        )
        .unwrap();
        let d = dir.display();
        let adapter = swarm::adapter::parse(
            "fake",
            &format!(
                "self = true\nspawn = true\nlist = true\nclose = true\n\
                 capture = n=$(($(cat '{d}/reads' 2>/dev/null || echo 0) + 1)); echo $n > '{d}/reads'; \
                 [ $n = 3 ] && cp '{d}/empty' '{d}/screen'; echo read >> '{d}/log'; cat '{d}/screen'\n\
                 ring = echo ring >> '{d}/log'; printf '────\\n%s\\n────\\n~/project\\n' \"$SWARM_TEXT\" > '{d}/screen'\n\
                 key = echo \"$SWARM_KEY\" >> '{d}/log'; cp '{d}/working' '{d}/screen'\n"
            ),
        )
        .unwrap();
        type_text(&connection, &adapter, &session, CODER, "%2", "hi").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "read\nread\nread\nring\nread\nEnter\nread\n"
        );
        // A pi that drew its box before the send takes the text and its Enter, so no read follows.
        std::fs::remove_file(dir.join("log")).unwrap();
        std::fs::copy(dir.join("empty"), dir.join("screen")).unwrap();
        type_text(&connection, &adapter, &session, CODER, "%2", "hi").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "read\nring\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deliver_rings_only_when_no_unread_is_rung() {
        let root = std::env::temp_dir().join(format!("swarm-deliver-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            format!(
                "self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
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
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();

        let first = deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "first",
        )
        .unwrap();
        let second = deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "second",
        )
        .unwrap();
        assert_eq!((first, second), (0, 1));
        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);

        ack(&mut connection, &root, "fake", &session, CODER, first).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
        ack(&mut connection, &root, "fake", &session, CODER, second).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "third",
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(3));

        // The coder listed "third" but has not acked it, so "fourth" is news and must ring.
        swarm::store::inbox(&connection, &session, CODER).unwrap();
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "fourth",
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(4));
    }

    /// A session whose coder pane `%2` shows the file `<root>/screen`, which the fake adapter's
    /// `capture` reads; `verbs` adds the `ring` verb and any other, with `{screen}` for that path.
    fn ring_session(
        name: &str,
        provider: Option<&str>,
        verbs: &str,
        screen: &str,
    ) -> (std::path::PathBuf, rusqlite::Connection, String) {
        let root = std::env::temp_dir().join(format!("swarm-{name}-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("adapters")).unwrap();
        std::fs::write(root.join("screen"), screen).unwrap();
        let verbs = verbs.replace("{screen}", &root.join("screen").display().to_string());
        std::fs::write(
            root.join("adapters/fake.conf"),
            format!(
                "self = true\nspawn = true\nlist = echo %1 %2\nclose = true\n\
                 capture = cat '{}'\n{verbs}\n",
                root.join("screen").display()
            ),
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        if let Some(provider) = provider {
            swarm::store::set_provider(&connection, &session, CODER, provider).unwrap();
        }
        (root, connection, session)
    }

    fn delivery_of(connection: &rusqlite::Connection, session: &str, seq: i64) -> Option<String> {
        swarm::store::messages(connection, session, -1)
            .unwrap()
            .into_iter()
            .find(|message| message.seq == seq)
            .unwrap()
            .delivery
    }

    fn send_task(
        root: &std::path::Path,
        connection: &mut rusqlite::Connection,
        session: &str,
    ) -> i64 {
        deliver(
            connection,
            root,
            "fake",
            session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap()
    }

    /// A ring that starts no turn is stored as unconfirmed at its deadline, for each provider, and
    /// the send still succeeds (ADR 0041).
    #[test]
    fn a_ring_that_starts_no_turn_is_unconfirmed_at_its_deadline() {
        let runs = [
            (
                "claude",
                include_str!("../tests/fixtures/screens/claude-idle.txt"),
            ),
            (
                "codex",
                include_str!("../tests/fixtures/screens/codex-idle.txt"),
            ),
            (
                "agy",
                include_str!("../tests/fixtures/screens/agy-idle.txt"),
            ),
        ]
        .map(|(provider, screen)| {
            std::thread::spawn(move || {
                let (root, mut connection, session) = ring_session(
                    "unconfirmed",
                    Some(provider),
                    "ring = true\nkey = true",
                    screen,
                );
                let started = std::time::Instant::now();
                let seq = send_task(&root, &mut connection, &session);
                assert!(
                    started.elapsed() >= RING_TIMEOUT - std::time::Duration::from_secs(1),
                    "{provider}"
                );
                assert_eq!(
                    delivery_of(&connection, &session, seq).as_deref(),
                    Some("unconfirmed"),
                    "{provider}"
                );
                std::fs::remove_dir_all(root).unwrap();
            })
        });
        for run in runs {
            run.join().unwrap();
        }
    }

    /// A working screen after the ring proves it. A pane with no provider and no Herdr status
    /// cannot prove a ring, so it is stored as unchecked with no wait.
    #[test]
    fn a_working_screen_proves_a_ring_and_a_pane_with_no_reader_is_unchecked() {
        let (root, mut connection, session) = ring_session(
            "screen-proof",
            Some("agy"),
            "ring = cp \"{screen}.working\" \"{screen}\"",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        std::fs::write(
            root.join("screen.working"),
            include_str!("../tests/fixtures/screens/agy-working.txt"),
        )
        .unwrap();
        let seq = send_task(&root, &mut connection, &session);
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("screen")
        );
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut connection, session) =
            ring_session("unchecked", None, "ring = true", "anything\n");
        let started = std::time::Instant::now();
        let seq = send_task(&root, &mut connection, &session);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("unchecked")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A failed read is no proof yet. A screen verb that fails does not make a pane with no
    /// provider unchecked, and a capture that keeps failing still lets the deadline settle a ring.
    #[test]
    fn a_failed_read_proves_nothing_and_the_ring_still_meets_its_deadline() {
        let (root, mut connection, session) = ring_session(
            "screen-fails",
            None,
            "ring = true\nscreen = false",
            "anything\n",
        );
        let seq = send_task(&root, &mut connection, &session);
        assert_eq!(
            delivery_of(&connection, &session, seq).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut connection, session) =
            ring_session("capture-fails", Some("claude"), "ring = true", "");
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = 1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        std::fs::remove_file(root.join("screen")).unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Wait,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A pass whose batch deadline is spent cannot read the pane. That is no evidence, so a late
    /// ring with no proof in the store stays for a pass with time to read its screen.
    #[test]
    fn a_pass_out_of_time_leaves_a_late_ring_unsettled() {
        let (root, mut connection, session) = ring_session(
            "settle-no-time",
            Some("agy"),
            "ring = true",
            include_str!("../tests/fixtures/screens/agy-working.txt"),
        );
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = 1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        let adapter = swarm::adapter::Adapter {
            deadline: Some(std::time::Instant::now()),
            ..swarm::adapter::load(&root, "fake").unwrap()
        };
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Later,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(delivery_of(&connection, &session, ask), None);

        // A pass with time reads the working screen that a busy CLI shows (ADR 0038).
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Later,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("screen")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A failed capture hides only the screen. The store still holds a turn-start hook or a read
    /// after the ring, so a late pass stores that proof, not unconfirmed (ADR 0041).
    #[test]
    fn a_failed_capture_still_finds_the_hook_or_the_read_in_the_store() {
        let proofs = [
            (
                "hook",
                "UPDATE agent SET state = 'working', state_source = 'hook', state_at = unixepoch()",
            ),
            ("seen", "UPDATE message SET seen_at = unixepoch()"),
        ];
        for (proof, update) in proofs {
            let (root, mut connection, session) =
                ring_session("capture-fails-proof", Some("claude"), "ring = true", "");
            let ask = swarm::store::send_message(
                &mut connection,
                &root,
                &session,
                ORCHESTRATOR,
                CODER,
                "ask",
                "task",
            )
            .unwrap();
            connection
                .execute(
                    "UPDATE message SET rung_at = unixepoch() - ?1, rings = 1",
                    [RING_TIMEOUT.as_secs() + 1],
                )
                .unwrap();
            connection.execute(update, []).unwrap();
            std::fs::remove_file(root.join("screen")).unwrap();
            let adapter = swarm::adapter::load(&root, "fake").unwrap();
            settle_rings(
                &mut connection,
                &root,
                &adapter,
                &session,
                Proof::Wait,
                &mut Vec::new(),
            )
            .unwrap();
            assert_eq!(
                delivery_of(&connection, &session, ask).as_deref(),
                Some(proof)
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    /// A store error while reading a ring's proof proves nothing either way. The waiting ring and
    /// a late pass store no result, so the chair is not told that a delivered message was lost.
    #[test]
    fn a_store_error_in_the_proof_check_stores_no_ring_result() {
        let (root, mut connection, session) = ring_session(
            "proof-store-error",
            Some("claude"),
            "ring = true",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
        );
        let seq = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        let rung_at = unix_now().unwrap();
        connection
            .execute("UPDATE message SET rung_at = ?1, rings = 1", [rung_at])
            .unwrap();
        // A temp `agent` with no `state_source` shadows the real table, so the hook read fails
        // and the ring's other agent reads still work.
        connection
            .execute_batch(
                "CREATE TEMP TABLE agent AS SELECT * FROM main.agent;
                 ALTER TABLE temp.agent DROP COLUMN state_source;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let delivery = ring_and_record(
            &connection,
            &root,
            Ok(adapter.clone()),
            &session,
            CODER,
            "%2",
            &[seq],
            rung_at,
            Proof::Wait,
        );
        assert_eq!(delivery, None);
        assert_eq!(delivery_of(&connection, &session, seq), None);

        connection
            .execute(
                "UPDATE message SET rung_at = ?1",
                [rung_at - RING_TIMEOUT.as_secs() as i64 - 1],
            )
            .unwrap();
        assert!(
            settle_rings(
                &mut connection,
                &root,
                &adapter,
                &session,
                Proof::Wait,
                &mut Vec::new()
            )
            .is_err()
        );
        assert_eq!(delivery_of(&connection, &session, seq), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Two sweeps can read the same unsettled ring of the chair. Only the pass whose write stores
    /// the result prints the chair's line, so the line comes once.
    #[test]
    fn a_pass_that_lost_the_race_to_settle_the_chairs_ring_prints_no_line() {
        let (root, mut connection, session) = ring_session(
            "settle-race",
            Some("agy"),
            "ring = true",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
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
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = ?2",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        // The other pass stored its result first, so this pass's guarded write matches no row.
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER other_pass BEFORE UPDATE OF delivery ON main.message
                 BEGIN SELECT RAISE(IGNORE); END;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = Vec::new();
        settle_rings(
            &mut connection,
            &root,
            &adapter,
            &session,
            Proof::Wait,
            &mut lines,
        )
        .unwrap();
        assert!(lines.is_empty(), "{lines:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A store error on a later ring fails the sweep pass, but the chair's own lost-ring line that
    /// the pass already stored is still printed: no later pass finds that ring again.
    #[test]
    fn a_failed_settle_still_prints_the_chairs_lost_ring_it_stored() {
        let (root, mut connection, session) = ring_session(
            "settle-error",
            Some("agy"),
            "ring = true",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        swarm::store::add_agent(&connection, &session, "reviewer", "reviewer").unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            "reviewer",
            "ask",
            "review",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1, rings = ?2",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        // The reviewer's ring settles after the chair's, and its write fails.
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER store_error BEFORE UPDATE OF delivery ON main.message
                 WHEN NEW.recipient_id = 'reviewer'
                 BEGIN SELECT RAISE(ABORT, 'store error'); END;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = Vec::new();
        let pass = sweep_once(
            &mut connection,
            &root,
            &adapter,
            &session,
            ORCHESTRATOR,
            &mut lines,
        );
        assert!(pass.is_err());
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} {summary}")]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A store error while the sweep re-rings the chair comes before the ring, so the ring is
    /// left for the next pass, whose line names the chair's lost message.
    #[test]
    fn a_failed_chair_read_in_a_rering_leaves_the_lost_line_to_the_next_pass() {
        let (root, mut connection, session) = ring_session(
            "rering-error",
            Some("agy"),
            "ring = false",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1",
                [],
            )
            .unwrap();
        // A temp `agent` with no `role` shadows the real table, so only the chair read fails.
        connection
            .execute_batch(
                "CREATE TEMP TABLE agent AS SELECT * FROM main.agent;
                 ALTER TABLE temp.agent DROP COLUMN role;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = Vec::new();
        let pass = sweep_once(
            &mut connection,
            &root,
            &adapter,
            &session,
            ORCHESTRATOR,
            &mut lines,
        );
        assert!(pass.is_err());
        connection.execute_batch("DROP TABLE temp.agent").unwrap();
        lines.extend(swept(&mut connection, &root, &adapter, &session));
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} {summary}")]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A re-ring of the chair whose result write fails stores nothing, so it prints no line. The
    /// later pass that stores the result prints the chair's line, so the line comes once.
    #[test]
    fn a_rering_whose_result_was_not_stored_leaves_the_lost_line_to_the_pass_that_stores_it() {
        let (root, mut connection, session) = ring_session(
            "rering-unstored",
            Some("agy"),
            "ring = false",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1",
                [],
            )
            .unwrap();
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER store_error BEFORE UPDATE OF delivery ON main.message
                 WHEN NEW.delivery IS NOT NULL
                 BEGIN SELECT RAISE(ABORT, 'store error'); END;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let mut lines = swept(&mut connection, &root, &adapter, &session);
        connection
            .execute_batch("DROP TRIGGER temp.store_error")
            .unwrap();
        connection
            .execute(
                "UPDATE message SET rung_at = unixepoch() - ?1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        lines.extend(swept(&mut connection, &root, &adapter, &session));
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} {summary}")]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A ring whose Enter was lost sits in the input box. Swarm presses Enter again for each CLI
    /// whose box it can read, and the turn that starts proves the ring. AGY's box cannot be read,
    /// so it gets no second Enter.
    #[test]
    fn a_lost_enter_is_pressed_again_for_each_cli_whose_input_box_swarm_reads() {
        let claude_box = |text: &str| format!("────\n❯ {text}\n────\n");
        let runs = [
            ("claude", claude_box as fn(&str) -> String),
            ("codex", |text: &str| format!("• Done.\n\n› {text}\n\n  ? for shortcuts\n")),
            ("agy", |text: &str| format!("> {text}\n")),
        ]
        .map(|(provider, held)| {
            std::thread::spawn(move || {
                let (root, mut connection, session) = ring_session(
                    "lost-enter",
                    Some(provider),
                    "ring = echo ring >> \"{screen}.log\"; cp \"{screen}.held\" \"{screen}\"\n\
                     key = echo \"$SWARM_KEY\" >> \"{screen}.log\"; cp \"{screen}.working\" \"{screen}\"",
                    &held(""),
                );
                std::fs::write(root.join("screen.held"), held(&ring_text(&root))).unwrap();
                let working = match provider {
                    "claude" => format!("✻ Thinking…\n{}", claude_box("")),
                    _ => include_str!("../tests/fixtures/screens/codex-working.txt").to_string(),
                };
                std::fs::write(root.join("screen.working"), working).unwrap();
                let seq = send_task(&root, &mut connection, &session);
                let log = std::fs::read_to_string(root.join("screen.log")).unwrap();
                let delivery = delivery_of(&connection, &session, seq);
                std::fs::remove_dir_all(root).unwrap();
                (provider, log, delivery)
            })
        });
        let results: Vec<_> = runs.map(|run| run.join().unwrap()).into();
        assert_eq!(
            results,
            [
                (
                    "claude",
                    "ring\nEnter\n".to_string(),
                    Some("screen".to_string())
                ),
                (
                    "codex",
                    "ring\nEnter\n".to_string(),
                    Some("screen".to_string())
                ),
                ("agy", "ring\n".to_string(), Some("unconfirmed".to_string())),
            ]
        );
    }

    /// A report of a lost message that a pass cannot send, here because the chair has no pane yet,
    /// is sent by a later pass. Its message is past its last ring, so no ring would find it again.
    #[test]
    fn a_lost_message_report_that_failed_to_send_is_sent_by_a_later_pass() {
        let (root, mut connection, session) = ring_session(
            "lost-retry",
            Some("agy"),
            "ring = true",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 2, delivery = 'unconfirmed'",
                [],
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("unconfirmed {CODER} {ask}")]
        );
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("unconfirmed:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A report whose row is stored but whose ring mark fails is sent: the pass keeps its line,
    /// a later pass does not send it twice, and the sweep's re-ring rings the unrung row.
    #[test]
    fn a_stored_report_whose_ring_mark_failed_is_sent_and_rung_later() {
        let (root, mut connection, session) =
            ring_session("report-unrung", None, "ring = true", "anything\n");
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        connection
            .execute_batch(
                "CREATE TEMP TRIGGER store_error BEFORE UPDATE OF rung_at ON main.message
                 BEGIN SELECT RAISE(ABORT, 'store error'); END;",
            )
            .unwrap();
        let send = |connection: &mut rusqlite::Connection| {
            report(
                connection,
                &root,
                "fake",
                &session,
                CODER,
                "stall:silent:0",
                "coder sent nothing back",
                Proof::Wait,
            )
        };
        assert!(send(&mut connection));
        assert!(!send(&mut connection));
        assert_eq!(chair_mail(&connection, &session).len(), 1);

        connection
            .execute_batch(
                "DROP TRIGGER temp.store_error;
                 UPDATE message SET created_at = unixepoch() - 61;",
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        swept(&mut connection, &root, &adapter, &session);
        let rings: i64 = connection
            .query_row("SELECT rings FROM message", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rings, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// One sweep pass by the chair that must succeed, and its lines.
    fn swept(
        connection: &mut rusqlite::Connection,
        root: &std::path::Path,
        adapter: &swarm::adapter::Adapter,
        session: &str,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        sweep_once(connection, root, adapter, session, ORCHESTRATOR, &mut lines).unwrap();
        lines
    }

    /// The kinds and senders of the messages to the chair.
    fn chair_mail(connection: &rusqlite::Connection, session: &str) -> Vec<(String, String)> {
        swarm::store::messages(connection, session, -1)
            .unwrap()
            .into_iter()
            .filter(|message| message.recipient == ORCHESTRATOR)
            .map(|message| (message.sender, message.kind))
            .collect()
    }

    #[test]
    fn the_listing_hides_the_kind_when_the_wait_is_stale() {
        let (root, mut connection, session) = ring_session(
            "listing-stale-kind",
            Some("claude"),
            "ring = true\nscreen = true",
            "",
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "waiting",
            "hook",
            None,
            Some("permission"),
            now - 50,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let output =
            serde_json::to_value(list_agents(&mut connection, &root, &session, &adapter).unwrap())
                .unwrap();
        let coder = output["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == CODER)
            .unwrap();
        assert!(coder["state"].is_null());
        assert!(coder["state_kind"].is_null());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_listing_clears_the_kind_when_an_idle_screen_ends_the_wait() {
        let (root, mut connection, session) = ring_session(
            "listing-idle-kind",
            Some("claude"),
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "waiting",
            "hook",
            None,
            Some("permission"),
            now - 20,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let output =
            serde_json::to_value(list_agents(&mut connection, &root, &session, &adapter).unwrap())
                .unwrap();
        let coder = output["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == CODER)
            .unwrap();
        assert_eq!(coder["state"], "done");
        assert!(coder["state_kind"].is_null());
        let coder = swarm::store::agents(&connection, &session)
            .unwrap()
            .into_iter()
            .find(|row| row.id == CODER)
            .unwrap();
        assert_eq!(coder.state.as_deref(), Some("done"));
        assert_eq!(coder.state_kind, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_listing_uses_the_session_chair_provider_for_a_screen_gap() {
        let (root, mut connection, session) = ring_session(
            "listing-chair-gap",
            None,
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/codex-failed.txt"),
        );
        swarm::store::set_chair(&connection, &session, Some(("codex", "chair"))).unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        swarm::store::set_state(
            &connection,
            &session,
            ORCHESTRATOR,
            "working",
            "hook",
            None,
            None,
            now - 20,
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        let output =
            serde_json::to_value(list_agents(&mut connection, &root, &session, &adapter).unwrap())
                .unwrap();
        let chair = output["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == ORCHESTRATOR)
            .unwrap();
        assert!(chair["provider"].is_null());
        assert_eq!(chair["state"], "failed");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The app kills `agents --json` at 20 s, so the listing types its re-ring and returns. A later
    /// pass settles the ring from the store and the screen, and only then tells the chair that it
    /// started no turn (ADR 0041, owner answer 2).
    #[test]
    fn the_listing_types_its_rering_and_a_later_pass_settles_it() {
        let (root, mut connection, session) = ring_session(
            "listing-rering",
            Some("claude"),
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "claude").unwrap();
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61, rings = 1",
                [],
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        let started = std::time::Instant::now();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(started.elapsed() < RING_TIMEOUT - std::time::Duration::from_secs(2));
        assert_eq!(delivery_of(&connection, &session, ask), None);
        assert!(chair_mail(&connection, &session).is_empty());

        // The ring's deadline passes with no turn started.
        connection
            .execute(
                "UPDATE message SET rung_at = rung_at - ?1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        let started = std::time::Instant::now();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(started.elapsed() < RING_TIMEOUT - std::time::Duration::from_secs(2));
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("unconfirmed")
        );
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("unconfirmed:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The app drops the listing's stderr, so a listing leaves the chair's own lost ring to
    /// `swarm sweep`, whose line names it (ADR 0041).
    #[test]
    fn a_listing_leaves_the_chairs_lost_ring_to_the_sweep_line() {
        let (root, mut connection, session) = ring_session(
            "chair-lost-listing",
            Some("agy"),
            "ring = true",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("agy", "chair-id"))).unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - ?1, rung_at = unixepoch() - ?1,
                                    rings = ?2",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(delivery_of(&connection, &session, summary), None);
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("unconfirmed {ORCHESTRATOR} {summary}")]
        );
        assert_eq!(
            delivery_of(&connection, &session, summary).as_deref(),
            Some("unconfirmed")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A listing ring whose `ring` verb fails stores nothing, like any listing ring, so the
    /// chair's own lost ring still reaches the `swarm sweep` line (ADR 0041).
    #[test]
    fn a_listing_ring_that_fails_leaves_the_chairs_lost_ring_to_the_sweep_line() {
        let (root, mut connection, session) = ring_session(
            "chair-ring-fails",
            Some("claude"),
            "ring = false\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "claude").unwrap();
        let summary = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1",
                [],
            )
            .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(delivery_of(&connection, &session, summary), None);

        // The failed ring's deadline passes.
        connection
            .execute(
                "UPDATE message SET rung_at = rung_at - ?1",
                [RING_TIMEOUT.as_secs() + 1],
            )
            .unwrap();
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("unconfirmed {ORCHESTRATOR} {summary}")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A child that reads and acks its message and ends its turn between two listing passes leaves
    /// its hook at done and its screen idle, so neither proves the ring. The read proves it: the
    /// pass stores `seen`, sends no `unconfirmed` report, and counts the ring for the silent stall
    /// (ADR 0041, owner answer 3).
    #[test]
    fn a_ring_whose_message_was_read_is_seen_though_its_turn_has_ended() {
        let (root, mut connection, session) = ring_session(
            "seen",
            Some("claude"),
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "claude").unwrap();
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - ?1, rung_at = unixepoch() - ?1,
                                    rings = ?2, seen_at = unixepoch() - 5",
                (RING_TIMEOUT.as_secs() + 1, swarm::store::MAX_RINGS),
            )
            .unwrap();
        swarm::store::ack(&connection, &session, ask, CODER).unwrap();
        let now = unix_now().unwrap();
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

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(
            delivery_of(&connection, &session, ask).as_deref(),
            Some("seen")
        );
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("stall:silent:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// After the second ring of a message starts no turn, the chair gets one `unconfirmed`
    /// message from the agent, not silence. A lost ring to the chair itself is only a sweep line,
    /// because a message would ring the same lost pane a third time (ADR 0041).
    #[test]
    fn the_chair_hears_of_a_message_whose_second_ring_started_no_turn() {
        let (root, mut connection, session) = ring_session(
            "lost-ring",
            Some("agy"),
            "ring = true",
            include_str!("../tests/fixtures/screens/agy-idle.txt"),
        );
        // The chair's screen reads idle, so a report may ring it.
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_provider(&connection, &session, ORCHESTRATOR, "agy").unwrap();
        send_task(&root, &mut connection, &session);
        let backdate =
            "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61
             WHERE recipient_id = 'coder'";
        connection.execute(backdate, []).unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();

        let lines = swept(&mut connection, &root, &adapter, &session);
        assert_eq!(lines, ["unconfirmed coder 0"]);
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), "unconfirmed:0".to_string())]
        );
        let body = std::fs::read_to_string(root.join(format!("runs/{session}/1.txt"))).unwrap();
        assert_eq!(
            body,
            "message 0 to coder was not delivered: no turn started after 2 rings"
        );
        connection.execute(backdate, []).unwrap();
        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        assert_eq!(chair_mail(&connection, &session).len(), 1);

        // The chair's own lost message: a line, and no message to itself.
        connection.execute("DELETE FROM message", []).unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%2").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("agy", "chair-id"))).unwrap();
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
        connection
            .execute("UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61, rings = 1", [])
            .unwrap();
        swarm::store::clear_pane(&connection, &session, CODER).unwrap();
        let lines = swept(&mut connection, &root, &adapter, &session);
        assert_eq!(lines, [format!("unconfirmed {ORCHESTRATOR} 0")]);
        assert_eq!(chair_mail(&connection, &session).len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A listing reads every unarchived session, and a restarted tmux or Herdr server can give a
    /// stored pane id to another pane. So the listing sends a report only while the adapter lists
    /// the chair's pane.
    #[test]
    fn a_listing_sends_no_report_while_the_chairs_pane_is_not_listed() {
        let (root, mut connection, session) = ring_session(
            "stall-listing",
            None,
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("claude", "chair-id"))).unwrap();
        let ask = send_task(&root, &mut connection, &session);
        connection
            .execute_batch(
                "UPDATE message SET delivery = 'hook', rung_at = unixepoch() - 30,
                                    seen_at = unixepoch() - 20",
            )
            .unwrap();
        let now = unix_now().unwrap();
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

        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%9").unwrap();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(chair_mail(&connection, &session).is_empty());
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("stall:unacked:{ask}"))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A report rings the chair, and its Enter would answer a question on the chair's screen. So
    /// the listing and the sweep send a report only while the chair's screen reads idle, and a
    /// later pass sends it. A chair that `session new` registered has no provider on its row, so
    /// both read its screen with the session's chair provider.
    #[test]
    fn a_report_waits_until_the_chairs_screen_reads_idle() {
        let (root, mut connection, session) = ring_session(
            "report-idle",
            Some("claude"),
            "ring = true\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/claude-question.txt"),
        );
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        // `session new` in a Claude pane names the chair only on the session, not on its row.
        swarm::store::set_chair(&connection, &session, Some(("claude", "chair-id"))).unwrap();
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        connection
            .execute_batch(
                "UPDATE message SET delivery = 'hook', rings = 1, rung_at = unixepoch() - 30,
                                    seen_at = unixepoch() - 20",
            )
            .unwrap();
        let now = unix_now().unwrap();
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

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        assert!(chair_mail(&connection, &session).is_empty());

        let show = |screen: &str| std::fs::write(root.join("screen"), screen).unwrap();
        show(include_str!("../tests/fixtures/screens/claude-idle.txt"));
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert_eq!(
            chair_mail(&connection, &session),
            [(CODER.to_string(), format!("stall:unacked:{ask}"))]
        );

        swarm::store::ack(&connection, &session, ask, CODER).unwrap();
        show(include_str!(
            "../tests/fixtures/screens/claude-question.txt"
        ));
        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(swept(&mut connection, &root, &adapter, &session).is_empty());
        assert_eq!(chair_mail(&connection, &session).len(), 1);
        show(include_str!("../tests/fixtures/screens/claude-idle.txt"));
        assert_eq!(
            swept(&mut connection, &root, &adapter, &session),
            [format!("stall coder silent {ask}")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The listing reads the screens first, then rings and settles, which takes seconds. A
    /// question that comes up on the chair's screen in that time stops the report, because the
    /// listing reads the chair's screen again right before it.
    #[test]
    fn a_listing_reads_the_chairs_screen_again_before_a_report() {
        // The coder's re-ring stands in for the gap: the fake ring puts a question on the screen
        // that the chair's pane shows too.
        let (root, mut connection, session) = ring_session(
            "report-fresh-idle",
            Some("claude"),
            "ring = cp '{screen}.question' '{screen}'\nscreen = cat '{screen}'",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
        );
        std::fs::write(
            root.join("screen.question"),
            include_str!("../tests/fixtures/screens/claude-question.txt"),
        )
        .unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_chair(&connection, &session, Some(("claude", "chair-id"))).unwrap();
        let ask = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "task",
        )
        .unwrap();
        let follow_up = swarm::store::send_message(
            &mut connection,
            &root,
            &session,
            ORCHESTRATOR,
            CODER,
            "ask",
            "more",
        )
        .unwrap();
        connection
            .execute(
                "UPDATE message SET delivery = 'hook', rings = 1, rung_at = unixepoch() - 30,
                                    seen_at = unixepoch() - 20
                 WHERE seq = ?1",
                [ask],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE message SET created_at = unixepoch() - 61, rung_at = unixepoch() - 61,
                                    rings = 1
                 WHERE seq = ?1",
                [follow_up],
            )
            .unwrap();
        let now = unix_now().unwrap();
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

        list_agents(&mut connection, &root, &session, &adapter).unwrap();
        assert!(
            chair_mail(&connection, &session).is_empty(),
            "{:?}",
            chair_mail(&connection, &session)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// `swarm sweep` sends the chair one message for each stall kind of a child, and no second
    /// one for the same stall (ADR 0041).
    #[test]
    fn sweep_reports_each_stall_to_the_chair_once() {
        let (root, mut connection, session) = ring_session(
            "stall",
            None,
            "ring = true",
            include_str!("../tests/fixtures/screens/claude-idle.txt"),
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
    fn orchestrator_agent_add_records_self_pane_and_child_finish_rings_it() {
        let root = std::env::temp_dir().join(format!("swarm-chair-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let ring_log = root.join("rings");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            format!(
                "self = printf '%s' '%9'\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
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

        add_agent(
            &connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap();

        assert_eq!(
            swarm::store::pane_of(&connection, &session, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%9")
        );
        assert_eq!(
            std::fs::read_to_string(ring_log).unwrap(),
            format!("%9:{}\n", ring_text(&root))
        );
    }

    #[test]
    fn a_routing_role_chair_receives_a_child_finish() {
        let root =
            std::env::temp_dir().join(format!("swarm-routing-chair-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            format!(
                "self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
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
            Some("fake"),
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "code.complex").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%9").unwrap();

        let chair = swarm::store::orchestrator_of(&connection, &session).unwrap();
        deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            CODER,
            &chair,
            "summary",
            "done",
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(ring_log).unwrap(),
            format!("%9:{}\n", ring_text(&root))
        );
    }

    #[test]
    fn only_the_orchestrator_can_set_the_session_chair() {
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "code.complex").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();

        let error = set_chair_for_caller(&connection, &session, CODER, Some(("claude", "wrong")))
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "swarm: only the orchestrator can set the session chair"
        );
        set_chair_for_caller(
            &connection,
            &session,
            ORCHESTRATOR,
            Some(("claude", "right")),
        )
        .unwrap();
        let stored = swarm::store::sessions(&connection).unwrap();
        assert_eq!(stored[0].chair_id.as_deref(), Some("right"));
    }

    #[test]
    fn a_long_command_is_rung_as_a_short_line_that_sources_it() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-script-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let session = "0199a000-0000-7000-8000-000000000001";
        let command = format!("'claude' '--settings' '{}'", "x".repeat(2000));

        let line = script_line(&root, session, CODER, &command).unwrap();
        let script = root.join(format!("runs/{session}/{CODER}.sh"));
        assert_eq!(line, format!(". '{}'", script.display()));
        assert!(line.len() < 256);
        assert_eq!(
            std::fs::read_to_string(&script).unwrap(),
            format!("{command}\n")
        );
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&script), 0o600);
        assert_eq!(mode(script.parent().unwrap()), 0o700);

        std::fs::write(&script, "echo ran\n").unwrap();
        let shell = std::process::Command::new("sh")
            .args(["-c", &format!("{line}; echo after")])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&shell.stdout), "ran\nafter\n");
    }

    fn session_command(home: &std::path::Path, args: &[&str]) -> std::process::Output {
        let exe = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("swarm{}", std::env::consts::EXE_SUFFIX));
        std::process::Command::new(exe)
            .env_clear()
            .env("HOME", home)
            .env("SWARM_HOME", home)
            .args(args)
            .output()
            .unwrap()
    }

    #[test]
    fn session_archive_can_be_listed_and_reversed() {
        let (home, connection, first) = stop_fixture("archive-list", None);
        let second = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        let active = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        let archived = session_command(&home, &["session", "archive", &first, &second]);
        assert!(archived.status.success());
        let normal = session_command(&home, &["sessions", "--json"]);
        assert!(normal.status.success());
        let normal: serde_json::Value = serde_json::from_slice(&normal.stdout).unwrap();
        assert_eq!(normal["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(normal["sessions"][0]["id"], active);
        assert!(normal["sessions"][0].get("archivedAt").is_none());
        let all = session_command(&home, &["sessions", "--json", "--archived"]);
        assert!(
            all.status.success(),
            "{}",
            String::from_utf8_lossy(&all.stderr)
        );
        let all: serde_json::Value = serde_json::from_slice(&all.stdout).unwrap();
        let rows = all["sessions"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        for id in [&first, &second] {
            let row = rows.iter().find(|row| row["id"] == *id).unwrap();
            let stored: i64 = connection
                .query_row(
                    "SELECT archived_at FROM session WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(stored > 0);
            assert_eq!(row["archivedAt"], stored);
        }
        let restored = session_command(&home, &["session", "unarchive", &first, &second]);
        assert!(
            restored.status.success(),
            "{}",
            String::from_utf8_lossy(&restored.stderr)
        );
        let normal = session_command(&home, &["sessions", "--json"]);
        let normal: serde_json::Value = serde_json::from_slice(&normal.stdout).unwrap();
        assert_eq!(normal["sessions"].as_array().unwrap().len(), 3);
        for row in normal["sessions"].as_array().unwrap() {
            assert!(row.get("archivedAt").is_none());
            let stored: Option<i64> = connection
                .query_row(
                    "SELECT archived_at FROM session WHERE id = ?1",
                    [row["id"].as_str().unwrap()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(stored, None);
        }
        assert!(
            session_command(&home, &["session", "unarchive", &first])
                .status
                .success()
        );
    }

    #[test]
    fn session_archived_listing_keeps_active_rows_and_limits_archives_in_archive_order() {
        let (home, connection, first_active) = stop_fixture("archive-limit", None);
        let second_active =
            swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        let mut archived_ids = Vec::new();
        for index in 0..55 {
            let id = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
            connection
                .execute(
                    "UPDATE session SET archived_at = ?1, created_at = ?2 WHERE id = ?3",
                    (10000 + index, 20000 - index, &id),
                )
                .unwrap();
            archived_ids.push(id);
        }
        let all = session_command(&home, &["sessions", "--json", "--archived"]);
        assert!(
            all.status.success(),
            "{}",
            String::from_utf8_lossy(&all.stderr)
        );
        let all: serde_json::Value = serde_json::from_slice(&all.stdout).unwrap();
        let rows = all["sessions"].as_array().unwrap();
        assert_eq!(rows.len(), 52);
        assert!(rows[..2].iter().all(|row| row.get("archivedAt").is_none()));
        assert!(rows[..2].iter().any(|row| row["id"] == first_active));
        assert!(rows[..2].iter().any(|row| row["id"] == second_active));
        for (row, id) in rows[2..].iter().zip(archived_ids[5..].iter().rev()) {
            assert_eq!(row["id"], *id);
        }
        assert_eq!(rows[2]["archivedAt"], 10054);
        assert_eq!(rows.last().unwrap()["archivedAt"], 10005);
        drop(connection);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn session_unarchive_names_unknown_id_and_rolls_back() {
        let (home, mut connection, known) = stop_fixture("unarchive-unknown", None);
        swarm::store::archive_sessions(&mut connection, std::slice::from_ref(&known)).unwrap();
        let unknown = uuid::Uuid::now_v7().to_string();
        let output = session_command(&home, &["session", "unarchive", &known, &unknown]);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains(&format!("swarm: no session {unknown}"))
        );
        let archived: Option<i64> = connection
            .query_row(
                "SELECT archived_at FROM session WHERE id = ?1",
                [&known],
                |row| row.get(0),
            )
            .unwrap();
        assert!(archived.is_some());
    }

    fn stop_fixture(
        tag: &str,
        log: Option<&str>,
    ) -> (std::path::PathBuf, rusqlite::Connection, String) {
        let home = std::env::temp_dir().join(format!("swarm-stop-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".swarm")).unwrap();
        let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", &home, None, None).unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        if let Some(text) = log {
            let path = home.join("chat.jsonl");
            std::fs::write(&path, text).unwrap();
            swarm::store::set_log(&connection, &session, CODER, &path).unwrap();
        }
        (home, connection, session)
    }

    fn run_stop_hook(
        home: &std::path::Path,
        session: &str,
        provider: &str,
        event: &str,
        payload: &str,
    ) {
        use std::io::Write;
        use std::process::{Command, Stdio};
        // Cargo builds this executable for the integration tests in the same test run.
        let exe = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("swarm{}", std::env::consts::EXE_SUFFIX));
        let mut child = Command::new(exe)
            .env_clear()
            .env("HOME", home)
            .env("SWARM_HOME", home)
            .env("SWARM_SESSION_ID", session)
            .env("SWARM_AGENT_ID", CODER)
            .args(["hook", provider, event])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"{}\n");
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn stop_hook_claude_replaces_cumulative_cost_and_model() {
        let log = format!(
            "{}\n{}\n{}\n{}\n",
            include_str!("../packages/transcript/src/fixtures/claude-chair-run.jsonl"),
            r#"{"type":"cost-state","totalCostUSD":1.25,"hasUnknownModelCost":false}"#,
            r#"{"type":"assistant","message":{"model":"claude-new"}}"#,
            r#"{"type":"cost-state","totalCostUSD":2.5,"hasUnknownModelCost":false}"#
        );
        let (home, connection, session) = stop_fixture("claude", Some(&log));
        for _ in 0..2 {
            run_stop_hook(&home, &session, "claude", "Stop", "{}");
            let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
            assert_eq!(agent.cost_usd, Some(2.5));
            assert_eq!(agent.model.as_deref(), Some("claude-new"));
            assert_eq!(agent.tokens, None);
        }
        std::fs::write(
            home.join("chat.jsonl"),
            r#"{"type":"cost-state","totalCostUSD":0,"hasUnknownModelCost":false}"#,
        )
        .unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.cost_usd, Some(0.0));
        assert_eq!(agent.model.as_deref(), Some("claude-new"));
    }

    #[test]
    fn stop_hook_commits_done_state_before_usage() {
        let log = r#"{"type":"assistant","message":{"id":"m1","model":"claude","usage":{"input_tokens":1,"output_tokens":2}}}"#;
        let (home, connection, session) = stop_fixture("state-before-usage", Some(log));
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "working",
            "hook",
            None,
            None,
            1,
        )
        .unwrap();
        // Reject a usage write unless the done state is already stored in the same row.
        connection.execute_batch("CREATE TRIGGER usage_requires_done BEFORE UPDATE OF usage_state ON agent WHEN NEW.state IS NOT 'done' BEGIN SELECT RAISE(ABORT, 'usage before done'); END;").unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.state.as_deref(), Some("done"));
        assert_eq!(agent.tokens, Some(3));
    }

    #[test]
    fn stop_hook_claude_adds_only_new_messages_in_a_growing_fixture() {
        use std::io::Write;
        let first = r#"{"type":"assistant","message":{"id":"m1","model":"claude","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":4}}}"#;
        let second = first.replace("m1", "m2");
        let log = format!(
            "{}\n{first}\n{first}\n",
            include_str!("../packages/transcript/src/fixtures/claude-chair-run.jsonl")
        );
        let (home, connection, session) = stop_fixture("claude-growing", Some(&log));
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        assert_eq!(
            swarm::store::agents(&connection, &session).unwrap()[0].tokens,
            Some(64)
        );
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(home.join("chat.jsonl"))
            .unwrap();
        writeln!(file, "{second}\n{first}\n{second}").unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.tokens, Some(128));
        assert_eq!(agent.cost_usd, None);
        assert_eq!(agent.state.as_deref(), Some("done"));
    }

    #[test]
    fn stop_hook_codex_uses_total_tokens_and_latest_model() {
        let log = format!(
            "{}\n{}\n{}\n{}\n",
            include_str!("../packages/transcript/src/fixtures/codex-chair-run.jsonl"),
            r#"{"type":"turn_context","payload":{"model":"gpt-old"}}"#,
            r#"{"type":"turn_context","payload":{"model":"gpt-new"}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":12},"total_token_usage":{"input_tokens":90,"output_tokens":10,"total_tokens":100}}}}"#
        );
        let (home, connection, session) = stop_fixture("codex", Some(&log));
        run_stop_hook(&home, &session, "codex", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.tokens, Some(100));
        assert_eq!(agent.model.as_deref(), Some("gpt-new"));
        assert_eq!(agent.cost_usd, None);
    }

    #[test]
    fn stop_hook_claude_reads_past_a_mebibyte_and_keeps_cost_without_a_complete_record() {
        let snapshot = r#"{"type":"cost-state","totalCostUSD":4,"hasUnknownModelCost":false}"#;
        let log = format!(
            "{snapshot}\n{}\n{snapshot}\n",
            "x".repeat(1024 * 1024 + 100)
        );
        let (home, connection, session) = stop_fixture("tail", Some(&log));
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        assert_eq!(
            swarm::store::agents(&connection, &session).unwrap()[0].cost_usd,
            Some(4.0)
        );
        // A cost record without a completeness flag does not replace a reported cost.
        std::fs::write(
            home.join("chat.jsonl"),
            format!(
                "{{\"type\":\"cost-state\",\"totalCostUSD\":9}}\n{}",
                "x".repeat(1024 * 1024 + 100)
            ),
        )
        .unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        assert_eq!(
            swarm::store::agents(&connection, &session).unwrap()[0].cost_usd,
            Some(4.0)
        );
    }

    #[test]
    fn stop_hook_missing_unreadable_or_snapshot_free_log_keeps_values() {
        let (home, connection, session) = stop_fixture("no-snapshot", Some("not json\n{}\n"));
        connection
            .execute(
                "UPDATE agent SET model = 'kept', cost_usd = 3, tokens = 5 WHERE session_id = ?1 AND id = ?2",
                (&session, CODER),
            )
            .unwrap();
        for mode in ["no-snapshot", "missing", "unreadable"] {
            if mode == "missing" {
                std::fs::remove_file(home.join("chat.jsonl")).unwrap();
            } else if mode == "unreadable" {
                use std::os::unix::fs::PermissionsExt;
                std::fs::write(
                    home.join("chat.jsonl"),
                    r#"{"type":"cost-state","totalCostUSD":9}"#,
                )
                .unwrap();
                std::fs::set_permissions(
                    home.join("chat.jsonl"),
                    std::fs::Permissions::from_mode(0o000),
                )
                .unwrap();
                assert!(std::fs::File::open(home.join("chat.jsonl")).is_err());
            }
            run_stop_hook(&home, &session, "claude", "Stop", "{}");
            let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
            assert_eq!(agent.model.as_deref(), Some("kept"));
            assert_eq!(agent.cost_usd, Some(3.0));
            assert_eq!(agent.tokens, Some(5));
            assert_eq!(agent.state.as_deref(), Some("done"));
        }
    }

    #[test]
    fn stop_hook_other_provider_interrupt_and_subagent_keep_usage() {
        let log = r#"{"type":"cost-state","totalCostUSD":9}"#;
        let (home, connection, session) = stop_fixture("other-provider", Some(log));
        connection
            .execute(
                "UPDATE agent SET cost_usd = 3 WHERE session_id = ?1 AND id = ?2",
                (&session, CODER),
            )
            .unwrap();
        for (provider, event, payload) in [
            ("agy", "Stop", "{}"),
            ("codex", "Interrupt", "{}"),
            ("claude", "Stop", r#"{"agent_id":"subagent"}"#),
        ] {
            run_stop_hook(&home, &session, provider, event, payload);
            assert_eq!(
                swarm::store::agents(&connection, &session).unwrap()[0].cost_usd,
                Some(3.0)
            );
        }
    }

    #[test]
    fn launch_fields_survive_a_fresh_connection_and_listing() {
        let root = std::env::temp_dir().join(format!("swarm-runner-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("bus.sqlite");
        let connection = swarm::store::open(&db).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        let adapter = swarm::adapter::parse("fake", "self = true\nspawn = printf '%s' '%2'\nring = true\nlist = true\nclose = true\ncapture = true\n").unwrap();
        let runner = swarm::config::Runner {
            provider: Provider::Codex,
            model: "gpt-6".into(),
            effort: "high".into(),
            sandbox: None,
            approval: None,
            permission: None,
        };
        register_spawned_pane(
            &connection,
            &adapter,
            &session,
            CODER,
            "code.complex",
            Some("codex"),
            &[],
            Some((&runner, Some("work"))),
        )
        .unwrap();
        let profile: String = connection
            .query_row("SELECT profile FROM agent WHERE id = ?1", [CODER], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(profile, "code.complex");
        drop(connection);
        let mut fresh = swarm::store::open(&db).unwrap();
        let output =
            serde_json::to_value(list_agents(&mut fresh, &root, &session, &adapter).unwrap())
                .unwrap();
        let agent = &output["agents"][0];
        assert_eq!(agent["profile"], "code.complex");
        assert_eq!(agent["runner"], "codex/gpt-6/high");
        assert_eq!(agent["model"], "gpt-6");
        assert_eq!(agent["effort"], "high");
        assert_eq!(agent["account"], "work");
        assert!(agent.get("costUsd").is_none());
        assert!(agent.get("tokens").is_none());
    }

    #[test]
    fn a_failed_adapter_spawn_rolls_back_the_agent() {
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();
        let failed = swarm::adapter::parse(
            "failed",
            "self = true\nspawn = echo refused >&2; exit 1\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();

        assert!(
            register_spawned_pane(
                &connection,
                &failed,
                &session,
                CODER,
                "coder",
                None,
                &[],
                None
            )
            .is_err()
        );
        assert!(
            swarm::store::agents(&connection, &session)
                .unwrap()
                .is_empty()
        );

        let working = swarm::adapter::parse(
            "working",
            "self = true\nspawn = printf '%s' '%2'\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        assert_eq!(
            register_spawned_pane(
                &connection,
                &working,
                &session,
                CODER,
                "coder",
                None,
                &[],
                None
            )
            .unwrap(),
            "%2"
        );
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

    #[test]
    fn a_failed_summary_delivery_keeps_the_log_and_releases_the_job() {
        let root =
            std::env::temp_dir().join(format!("swarm-summary-retry-test-{}", std::process::id()));
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
        let log = root.join(format!("runs/{session}/{CODER}.log"));
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "captured output").unwrap();
        let job = swarm::store::enqueue_job(&connection, &session, CODER, "summarize").unwrap();
        assert_eq!(swarm::store::claim_next(&connection).unwrap(), Some(job));

        let result = process_summary_job(&mut connection, &root, "fake", "cat", job).unwrap();

        assert!(result.contains("orchestrator has no pane"));
        assert!(log.exists());
        let state: String = connection
            .query_row("SELECT state FROM job WHERE id = ?1", [job], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(state, "queued");
    }

    #[test]
    fn default_codex_trust_uses_codex_home_or_login_home() {
        let root =
            std::env::temp_dir().join(format!("swarm-default-codex-test-{}", std::process::id()));
        let login = root.join("login");
        let codex = default_codex_home_with_env(|name| match name {
            "HOME" => Some(login.clone().into_os_string()),
            _ => None,
        })
        .unwrap();
        let store = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        swarm::bus::write_trust(&store, || {
            swarm::bus::codex_trust_plan(&codex, std::path::Path::new("/project"))
        })
        .unwrap();
        assert!(login.join(".codex/config.toml").exists());

        let override_home = root.join("override");
        assert_eq!(
            default_codex_home_with_env(
                |name| (name == "CODEX_HOME").then(|| override_home.clone().into_os_string())
            )
            .unwrap(),
            override_home
        );
    }

    #[test]
    fn a_chair_pane_moves_to_the_new_session() {
        let root =
            std::env::temp_dir().join(format!("swarm-chair-move-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = printf '%s' '%1'\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let first = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/first"),
            None,
            None,
        )
        .unwrap();
        let second = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/second"),
            None,
            None,
        )
        .unwrap();

        add_agent(
            &connection,
            &root,
            "fake",
            &first,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap();
        add_agent(
            &connection,
            &root,
            "fake",
            &second,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap();

        assert_eq!(
            swarm::store::pane_of(&connection, &first, ORCHESTRATOR).unwrap(),
            None
        );
        assert_eq!(
            swarm::store::pane_of(&connection, &second, ORCHESTRATOR)
                .unwrap()
                .as_deref(),
            Some("%1")
        );
    }

    #[test]
    fn orchestrator_registration_sets_the_session_adapter() {
        let root =
            std::env::temp_dir().join(format!("swarm-chair-adapter-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = printf '%s' '%1'\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            Some("tmux"),
        )
        .unwrap();

        add_agent(
            &connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap();

        let stored = swarm::store::sessions(&connection).unwrap();
        assert_eq!(stored[0].adapter.as_deref(), Some("fake"));
    }

    #[test]
    fn refused_orchestrator_registration_leaves_no_agent() {
        let root =
            std::env::temp_dir().join(format!("swarm-empty-chair-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            std::path::Path::new("/test"),
            None,
            None,
        )
        .unwrap();

        let error = add_agent(
            &connection,
            &root,
            "fake",
            &session,
            ORCHESTRATOR,
            "orchestrator",
        )
        .unwrap_err()
        .to_string();

        assert_eq!(error, "swarm: adapter gave no pane for orchestrator");
        assert!(
            swarm::store::agents(&connection, &session)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn deliver_to_agent_without_pane_writes_no_message() {
        let root = std::env::temp_dir().join(format!("swarm-no-pane-test-{}", std::process::id()));
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

        let error = deliver(
            &mut connection,
            &root,
            "fake",
            &session,
            CODER,
            ORCHESTRATOR,
            "summary",
            "done",
        )
        .unwrap_err()
        .to_string();

        assert_eq!(
            error,
            "swarm: orchestrator has no pane; nothing would ring it"
        );
        assert_eq!(
            swarm::store::messages(&connection, &session, -1)
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn deliver_from_or_to_an_unknown_agent_writes_no_message() {
        let root =
            std::env::temp_dir().join(format!("swarm-unknown-agent-test-{}", std::process::id()));
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
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();

        for (sender, recipient) in [("ghost", ORCHESTRATOR), (ORCHESTRATOR, "ghost")] {
            let error = deliver(
                &mut connection,
                &root,
                "fake",
                &session,
                sender,
                recipient,
                "ask",
                "hi",
            )
            .unwrap_err()
            .to_string();
            assert_eq!(
                error,
                format!("swarm: ghost is not an agent in session {session}")
            );
        }
        assert_eq!(
            swarm::store::messages(&connection, &session, -1)
                .unwrap()
                .len(),
            0
        );
    }
}
