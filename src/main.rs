use std::env;
use std::os::unix::process::ExitStatusExt;
use swarm::providers::Provider;

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
        let ready = std::time::Instant::now() + RING_READY_TIMEOUT;
        // A question on screen, such as a folder trust prompt, also ends the wait.
        while std::time::Instant::now() < ready {
            let rows = read();
            if swarm::screen::composer(provider, &rows).is_some()
                || swarm::screen::prompt(&rows).is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
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
        "swarm: to see Codex and AGY agents' chats and questions, run `swarm hooks setup`; it trusts swarm's own Codex hooks and adds swarm's AGY hooks"
    );

    Ok(())
}

const USAGE: &str = "usage: swarm --version | init | setup status --json | setup [--plan [--json] | --digest <digest>] [--cwd <dir>] [--only <hooks|trust|herdr>,...] [--consent <standing|ask>] [--resume] | hooks status --json | hooks setup [--plan [--json] | --digest <digest>] | managed list [--json] | managed revert (<id>... | --all) [--plan [--json] | --digest <digest>] | adapter check <name> | session new <talk_mode> [--chair <claude|codex>:<id>] (cwd: pwd -P) | session chair <claude|codex>:<id> | session continue <new_id> <old_id> | session archive <id>... | sessions --json | agent add <agent_id> <role> | herdr-split | host-context --provider <claude|codex|agy> | hook <claude|codex|agy> [event] | guard <claude|codex|agy> PreToolUse | roles --json | roles get <role> [--provider <claude|codex|agy>] | roles check --json | roles save --revision <revision> <profile-json> | providers --json | models --provider <claude|codex|agy> --json | accounts --provider <claude|codex|agy> --json | usage --json | agents --json [--all] | messages --json [--after <seq>] | launch <agent_id> <role> [--provider <claude|codex|agy>] [--model <model> for chat] [--account <auto|name>] [--cwd <dir>] [-- <provider args>...] | spawn <agent_id> <role> [--provider <p>] [--account <auto|name>] [-- <cmd>...] | type <agent_id> | answer <agent_id> <prompt_id> <choice> | interrupt <agent_id> | key <agent_id> <Up|C-u> | attach <agent_id> | close <agent_id> | send <recipient> <kind> | finish | exited | sweep [--every <secs>] | drain | inbox | ack <seq>";

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
    ["TMUX_PANE", "HERDR_PANE_ID"]
        .iter()
        .any(|name| env::var_os(name).is_some())
}

/// (session, agent, state, detail)
/// What one hook call reports: the session and agent, the state with its detail, and the chat log
/// path from the payload.
#[derive(Debug, PartialEq)]
struct HookReport {
    session: String,
    agent: String,
    state: Option<(&'static str, Option<String>)>,
    log: Option<std::path::PathBuf>,
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
    Ok(Some(HookReport {
        session: valid_session_id(&session)?,
        agent,
        state,
        log,
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
        swarm::store::set_log(&connection, &report.session, &report.agent, log)?;
    }
    let Some((state, detail)) = report.state else {
        return Ok(());
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    swarm::store::set_state(
        &connection,
        &report.session,
        &report.agent,
        state,
        "hook",
        detail.as_deref(),
        now,
    )
}

fn adapter_name() -> String {
    swarm::host::adapter(|name| env::var(name).ok())
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

/// `run_tool` that stops the tool after `limit`, for a read that a launch waits on.
fn run_tool_within(
    executable: &str,
    args: &[&str],
    limit: std::time::Duration,
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let mut child = std::process::Command::new(executable)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("swarm: cannot run {executable}: {error}"))?;
    let stdout = read_within(child.stdout.take().ok_or("no stdout")?, limit);
    let stderr = child.stderr.take().ok_or("no stderr")?;
    let status = match stdout.as_ref().map(|_| child.try_wait()) {
        Some(Ok(Some(status))) => Some(status),
        // stdout closed, so the tool is exiting; a short wait lets it finish.
        Some(_) => {
            std::thread::sleep(std::time::Duration::from_millis(50));
            child.try_wait().ok().flatten()
        }
        None => None,
    };
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!(
            "swarm: {executable} did not answer within {} s",
            limit.as_secs()
        )
        .into());
    };
    let stderr = read_within(stderr, std::time::Duration::from_millis(100)).unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout: stdout.unwrap_or_default().into_bytes(),
        stderr: stderr.into_bytes(),
    })
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

fn yelo_command() -> String {
    env::var("SWARM_YELO_CMD").unwrap_or_else(|_| "yelo".to_string())
}

/// Whether each runner can start now (ADR 0032). Each provider's accounts are read at most once.
struct Probe {
    min_usage_left_pct: u8,
    /// `--account <name>`: only that account counts. None or `auto` counts every account.
    account: Option<String>,
    accounts: std::cell::RefCell<
        std::collections::HashMap<Provider, Option<Vec<swarm::config::AccountState>>>,
    >,
}

impl Probe {
    fn new(config: &swarm::config::Config, account: Option<&str>) -> Probe {
        Probe {
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
            .or_insert_with(|| read_accounts(provider, self.account.as_deref()));
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

/// yelo's accounts for `provider`, or None when yelo is missing, fails, or takes over 2 s. The
/// read took 0.08 s on the owner's Mac; a launch must not wait on a stuck one.
fn read_accounts(
    provider: Provider,
    only: Option<&str>,
) -> Option<Vec<swarm::config::AccountState>> {
    let mut child = std::process::Command::new(yelo_command())
        .args([
            "profile",
            "list",
            "--cli",
            provider.id(),
            "--usage",
            "--json",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let text = read_within(child.stdout.take()?, std::time::Duration::from_secs(2));
    let _ = child.kill();
    let _ = child.wait();
    let list = swarm::profiles::translate_accounts(provider.id(), &text?, None).ok()?;
    Some(
        list.accounts
            .iter()
            .filter(|account| only.is_none_or(|name| account.name == name))
            .map(|account| swarm::config::AccountState {
                signed_in: account.signed_in,
                remaining_pct: account.remaining_pct,
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
) -> Result<serde_json::Value, String> {
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
    Ok(value)
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
    with_pick: bool,
) -> Result<swarm::profiles::AccountList, Box<dyn std::error::Error>> {
    match Provider::parse(provider) {
        None => return Err(format!("swarm: unknown provider {provider}").into()),
        Some(kind) if !kind.has_accounts() => {
            return Ok(swarm::profiles::empty_accounts(provider));
        }
        Some(_) => {}
    }
    // A launch waits on these reads, so a stuck yelo fails them rather than the launch hanging.
    let limit = std::time::Duration::from_secs(2);
    let command = yelo_command();
    let list = checked_stdout(
        &command,
        run_tool_within(
            &command,
            &["profile", "list", "--cli", provider, "--usage", "--json"],
            limit,
        )?,
    )?;
    let pick_json = if with_pick {
        let pick = run_tool_within(
            &command,
            &["profile", "pick", "--cli", provider, "--json"],
            limit,
        )?;
        pick.status
            .success()
            .then(|| String::from_utf8(pick.stdout))
            .transpose()
            .map_err(|error| format!("swarm: {command} printed non-UTF-8 output: {error}"))?
    } else {
        None
    };
    swarm::profiles::translate_accounts(provider, &list, pick_json.as_deref())
        .map_err(|error| format!("swarm: {error}").into())
}

fn print_json(value: &impl serde::Serialize) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

fn valid_chair_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn parse_chair(value: &str) -> Result<Option<(&str, &str)>, String> {
    let (provider, id) = value.split_once(':').ok_or_else(|| USAGE.to_string())?;
    if !matches!(provider, "claude" | "codex") {
        return Err(USAGE.to_string());
    }
    Ok(valid_chair_id(id).then_some((provider, id)))
}

fn env_chair() -> Option<(String, String)> {
    for (provider, variable) in [
        ("claude", "CLAUDE_CODE_SESSION_ID"),
        ("codex", "CODEX_THREAD_ID"),
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

/// Every Codex home a pane's `codex` may read: the default one and each yelo profile
/// (`~/.codex-<name>`).
fn codex_homes(
    user_home: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let mut homes = vec![default_codex_home()?];
    if let Ok(entries) = std::fs::read_dir(user_home) {
        homes.extend(
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_name().to_string_lossy().starts_with(".codex-")
                        && entry.path().is_dir()
                })
                .map(|entry| entry.path()),
        );
    }
    homes.sort();
    homes.dedup();
    Ok(homes)
}

/// `swarm hooks status --json | setup [--plan [--json] | --digest <digest>]`: whether swarm's own
/// Codex and AGY state hooks are set up, the plan of each change and conflict, and setting them up.
/// The owner consents first, in the app or by running `setup` (ADR 0029). Setup writes no file
/// while any entry conflicts with one the owner has, and `--digest` refuses a file that changed
/// after the plan (ADR 0036). Claude's state hooks need no step, because `swarm launch` passes
/// them with `--settings`. With a rule list, setup also registers `swarm guard` in every Claude
/// settings file, Codex `hooks.json` and its trust, and AGY's `hooks.json` (ADR 0040). It is
/// `swarm setup --only hooks` with its own text and JSON, kept for the brew CLI (ADR 0043).
fn hooks(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
    // The guard registrations go in only when the owner keeps a rule list (ADR 0040), so a Mac
    // with no list never gets a hook that would block every call.
    let has_list = swarm::paths::guards_file()?.exists();
    let files = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?;
    let plan = || hook_plans(&files, has_list);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["status", "--json"] => print_json(&serde_json::json!({
            "codex": files.homes.iter().all(|home| swarm::bus::codex_hooks_trusted(home, &codex)),
            "agy": swarm::bus::agy_hooks_set(&files.agy_hooks),
            "guard": files.guard_status(has_list),
        })),
        ["setup", "--plan"] => {
            print!(
                "{}",
                swarm::managed::plan_text(
                    &plan(),
                    "swarm hooks setup",
                    "swarm's hooks are already set up. No file changes."
                )
            );
            Ok(())
        }
        ["setup", "--plan", "--json"] => print_json(&swarm::managed::plan_json(&plan())),
        ["setup", rest @ ..] if matches!(rest, [] | ["--digest", _]) => {
            // Swarm makes no write that it cannot record (ADR 0042), so the store opens first.
            let store = swarm::store::open(&swarm::paths::sqlite_db()?)?;
            let lock = swarm::paths::trust_lock()?;
            swarm::managed::with_lock(&lock, || {
                let plans = plan();
                if let ["--digest", digest] = rest
                    && swarm::managed::digest(&plans) != *digest
                {
                    return Err(
                        "swarm: a hook file changed after the plan; check the plan again".into(),
                    );
                }
                for path in swarm::managed::apply(&store, &plans)? {
                    println!("swarm: set up swarm's hooks in {}", path.display());
                }
                Ok(())
            })?;
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// The files that `hooks setup` writes: each Codex home, AGY's `hooks.json`, and, for the guard,
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

    /// Every item that `hooks setup` adds, with swarm's current text, so `managed` finds one that
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
/// `hooks setup` is; `--plan` shows the diff first and `--digest` applies only that plan.
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

/// The plan of every file `hooks setup` writes. A file that swarm cannot read or edit is a
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

/// The groups of `swarm setup`, in plan order (ADR 0043). The list may grow, so the app shows a
/// group it does not know by its name.
const SETUP_GROUPS: [&str; 3] = ["hooks", "trust", "herdr"];

/// One `swarm setup` plan: each file's plan in apply order, the group of each, and each group
/// part that was left out with its reason.
struct SetupPlan {
    plans: Vec<swarm::managed::FilePlan>,
    groups: Vec<&'static str>,
    skipped: Vec<(&'static str, String)>,
}

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
            groups: Vec::new(),
            skipped: Vec::new(),
        };
        if groups.contains(&"hooks") {
            let has_list = swarm::paths::guards_file()?.exists();
            for plan in hook_plans(&files, has_list) {
                setup.push("hooks", plan);
            }
        }
        if groups.contains(&"trust") {
            setup.push("trust", consent_plan(consent)?);
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
                            .find(|plan| canonical(&plan.path) == canonical(path));
                        // A file the earlier plan cannot edit has no planned text to build on.
                        if let Some(conflict) = earlier.and_then(|plan| plan.conflicts.first()) {
                            return Err(conflict.found.clone());
                        }
                        let plan = match earlier.map(|plan| plan.after.clone()) {
                            Some(text) => swarm::bus::codex_trust_plan_on(path, text, &target)?,
                            None => swarm::bus::codex_trust_plan(path.parent().unwrap(), &target)?,
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
                        setup.push("trust", plan);
                        Ok(())
                    })?;
                    let settings = agy_settings(&user_home);
                    let plan = swarm::bus::agy_trust_plan(&settings, &target)
                        .unwrap_or_else(|error| FilePlan::unreadable(settings, error));
                    setup.push("trust", plan);
                    // A seat's Claude runs from the pool under cwd, or in cwd when it resumes.
                    let flags = if resume {
                        vec!["--resume".into()]
                    } else {
                        Vec::new()
                    };
                    let (dir, _) = swarm::bus::claude_child("", cwd, &flags, &uuid::Uuid::nil());
                    skipped.extend(trust_each(&claude_configs(&user_home), false, |config| {
                        setup.push("trust", swarm::bus::claude_trust_plan(config, &dir)?);
                        Ok(())
                    })?);
                    setup
                        .skipped
                        .extend(skipped.into_iter().map(|reason| ("trust", reason)));
                }
            }
        }
        // Hook point for the `herdr` group: swarm-notify's Herdr toast and sound writer (its ADR,
        // PR #30) adds its plans here; this build has none, so the group is always set up.
        Ok(setup)
    }

    fn push(&mut self, group: &'static str, plan: swarm::managed::FilePlan) {
        self.groups.push(group);
        self.plans.push(plan);
    }

    /// Each plan with its group.
    fn grouped(&self) -> impl Iterator<Item = (&'static str, &swarm::managed::FilePlan)> {
        self.groups.iter().copied().zip(&self.plans)
    }

    /// The digest that apply checks. Every running CLI rewrites a trust file such as
    /// `~/.claude.json`, so a trust plan's part is only each entry it adds and the value it
    /// replaces, and a change to one of those still changes the digest (L-4). Every other part is
    /// the file's whole text, as in `managed::digest`.
    fn digest(&self) -> String {
        use sha2::Digest;
        let mut digest = sha2::Sha256::new();
        for (group, plan) in self.grouped() {
            let mut parts = vec![plan.path.to_string_lossy().into_owned()];
            if group != "trust" {
                parts.extend([plan.before.clone(), plan.after.clone()]);
            }
            for edit in plan.edits.iter().filter(|_| group == "trust") {
                let before = edit.before.as_ref().map(serde_json::Value::to_string);
                parts.extend([edit.id(), edit.wrote.to_string(), format!("{before:?}")]);
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

/// `swarm setup status --json | setup [--plan [--json] | --digest <digest>] [--cwd <dir>]
/// [--only <group>,...] [--consent <standing|ask>] [--resume]`: every write swarm makes outside
/// its home, in one plan with one digest (ADR 0043). The groups are `hooks`, as `hooks setup`
/// writes them, `trust`, the launch consent (`--consent`, by default the recorded answer, else
/// standing) and the trust entries a launch in `--cwd` would write, for a resuming seat with
/// `--resume`, and `herdr`. Running it, or applying the plan's digest, is the owner's consent, as
/// for `hooks setup`, so a child pane may plan but not apply.
fn setup(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if args == ["status", "--json"] {
        let files = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?;
        let has_list = swarm::paths::guards_file()?.exists();
        return print_json(&serde_json::json!({
            "hooks": files.hooks_status(),
            "guard": files.guard_status(has_list),
            // The owner answered, standing or ask, so the app does not ask again.
            "trust": trust_answer().is_some(),
            "herdr": true,
        }));
    }
    let (mut plan, mut json, mut digest, mut cwd, mut only) = (false, false, None, None, None);
    let (mut consent, mut resume) = (None, false);
    let mut rest = args.iter().copied();
    while let Some(arg) = rest.next() {
        match arg {
            "--plan" => plan = true,
            "--json" => json = true,
            "--resume" => resume = true,
            "--digest" | "--cwd" | "--only" | "--consent" => {
                let value = Some(rest.next().ok_or(USAGE)?);
                match arg {
                    "--digest" => digest = value,
                    "--cwd" => cwd = value,
                    "--consent" => consent = value,
                    _ => only = value,
                }
            }
            _ => return Err(USAGE.into()),
        }
    }
    if (json && !plan)
        || (plan && digest.is_some())
        || consent.is_some_and(|answer| ![STANDING, ASK].contains(&answer))
    {
        return Err(USAGE.into());
    }
    // No --consent keeps the owner's answer, so the approve command a seat prints under `ask`
    // approves the folder and not standing consent; with no answer yet, setup offers standing.
    let answer = consent.unwrap_or_else(|| trust_answer().unwrap_or(STANDING));
    let groups: Vec<&str> = match only {
        None => SETUP_GROUPS.to_vec(),
        Some(list) => list.split(',').collect(),
    };
    if let Some(unknown) = groups.iter().find(|group| !SETUP_GROUPS.contains(group)) {
        return Err(format!(
            "swarm: unknown setup group {unknown}; groups: {}",
            SETUP_GROUPS.join(", ")
        )
        .into());
    }
    let dir = cwd.map_or_else(env::current_dir, |dir| Ok(dir.into()))?;
    let dir = std::fs::canonicalize(&dir)
        .map_err(|error| format!("swarm: bad --cwd {}: {error}", dir.display()))?;
    if plan {
        let setup = SetupPlan::of(&dir, &groups, answer, resume)?;
        let plan_digest = setup.digest();
        if json {
            let group_of = |value: serde_json::Value, group: &str| {
                let mut value = value;
                value["group"] = group.into();
                value
            };
            return print_json(&serde_json::json!({
                "digest": plan_digest,
                "consent": answer,
                "files": setup.grouped()
                    .filter(|(_, plan)| plan.after != plan.before)
                    .map(|(group, plan)| serde_json::json!({
                        "group": group,
                        "path": plan.path.to_string_lossy(),
                        "diff": swarm::managed::diff(plan),
                    }))
                    .collect::<Vec<_>>(),
                "conflicts": setup.grouped()
                    .flat_map(|(group, plan)| plan.conflicts.iter().map(move |conflict| (group, conflict)))
                    .map(|(group, conflict)| group_of(serde_json::json!(conflict), group))
                    .collect::<Vec<_>>(),
                "skipped": setup.skipped.iter()
                    .map(|(group, reason)| serde_json::json!({"group": group, "reason": reason}))
                    .collect::<Vec<_>>(),
            }));
        }
        let mut apply = format!("swarm setup --digest {plan_digest}");
        if let Some(cwd) = cwd {
            apply += &format!(" --cwd {}", swarm::adapter::shell_line(&[cwd.into()]));
        }
        if let Some(only) = only {
            apply += &format!(" --only {only}");
        }
        if let Some(consent) = consent {
            apply += &format!(" --consent {consent}");
        }
        if resume {
            apply += " --resume";
        }
        for (group, reason) in &setup.skipped {
            println!("skipped ({group}): {reason}");
        }
        print!(
            "{}",
            swarm::managed::plan_text(
                &setup.plans,
                &apply,
                "Swarm is already set up. No file changes."
            )
        );
        return Ok(());
    }
    if let Some(reason) = swarm::bus::setup_refusal(
        env::var("SWARM_AGENT_ID").ok().as_deref(),
        env::var("HERDR_AGENT_PANE").ok().as_deref(),
    ) {
        return Err(reason.into());
    }
    // Swarm makes no write that it cannot record (ADR 0042), so the store opens first.
    let store = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    swarm::managed::with_lock(&swarm::paths::trust_lock()?, || {
        let setup =
            SetupPlan::of(&dir, &groups, answer, resume).map_err(|error| error.to_string())?;
        let plans = &setup.plans;
        // A retry after a timeout finds nothing to do, and that is not a failure.
        if plans
            .iter()
            .all(|plan| plan.after == plan.before && plan.conflicts.is_empty())
        {
            println!("swarm: already set up. No file changes.");
            return Ok(());
        }
        if let Some(digest) = digest
            && setup.digest() != digest
        {
            return Err("swarm: a file changed after the plan; check the plan again".into());
        }
        if let Some(conflicts) = swarm::managed::conflicts_text(plans) {
            return Err(conflicts);
        }
        // A running CLI may rewrite its trust file at any moment, such as `~/.claude.json`, and
        // the digest covers only the trust entries, so each trust file is planned again and
        // written as launch's `write_trust` does (L-4).
        let trust_plan = |path: &std::path::Path| {
            SetupPlan::of(&dir, &["trust"], answer, resume)
                .map_err(|error| error.to_string())?
                .plans
                .into_iter()
                .find(|plan| plan.path == path)
                .ok_or_else(|| format!("swarm: {} changed after the plan", path.display()))
        };
        for (group, plan) in setup.grouped() {
            let written = if group == "trust" {
                swarm::bus::write_trust(&store, || trust_plan(&plan.path))
                    .map(|written| written.into_iter().map(|plan| plan.path).collect())
            } else {
                swarm::managed::apply(&store, std::slice::from_ref(plan))
            };
            match written {
                Ok(changed) => {
                    for path in changed {
                        println!("swarm: {group}: wrote {}", path.display());
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "{error}\nswarm: stopped; each file named above is written and recorded, see `swarm managed list`"
                    ));
                }
            }
        }
        Ok(())
    })?;
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
                    Ok(plan) => diffs += &swarm::managed::diff(&plan),
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
                    "swarm: approve with `swarm setup --plan --cwd {}{resume}`, then the `swarm setup --digest …` it prints.",
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
                eprint!("{}", swarm::managed::diff(plan));
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

/// Every Claude config a pane with no picked account may read: `~/.claude.json` and each yelo
/// profile's.
fn claude_configs(user_home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut configs = vec![user_home.join(".claude.json")];
    if let Ok(profiles) = std::fs::read_dir(user_home.join(".claude/.profiles")) {
        configs.extend(
            profiles
                .filter_map(Result::ok)
                // yelo keeps its own data in hidden dirs here; a profile is not hidden.
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
    command: &'a [String],
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
                    command: &args[index + 1..],
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
        command: &[],
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
) -> Result<(), Box<dyn std::error::Error>> {
    // Before any pane, file, or bus work: the id names a pane, a bus row, and a run script.
    if !swarm::bus::valid_agent_id(agent_id) {
        return Err(format!("swarm: bad agent id {agent_id}").into());
    }
    let account = if let Some(requested) = options.account {
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
        let accounts = load_accounts(&provider, true)?;
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
        let child = if let Some(account) = &account {
            let mut args = vec!["env".to_string(), "--".to_string()];
            args.extend(
                account
                    .env
                    .iter()
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
/// shell sources the file, so its functions (such as yelo's `codex`) still pick the account.
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
    let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?
        .ok_or("swarm: no pane recorded")?;
    swarm::adapter::load(&root, &adapter_name())?.attach(&[("pane", &pane)])
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
    let rows = swarm::store::agents(connection, session_id)?;
    let listing = match adapter.run("list", &[]) {
        Ok(listing) => Some(listing),
        Err(error) => {
            adapter.check_deadline()?;
            eprintln!("swarm: {}", error.to_string().replace(['\r', '\n'], " "));
            None
        }
    };
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
            .map(|(row, alive)| {
                let provider = if Some(&row.id) == chair.as_ref() {
                    chair_provider.as_deref()
                } else {
                    row.provider.as_deref()
                };
                let target = match (alive, row.pane.as_deref()) {
                    (Some(true), Some(pane)) => Some((pane, provider)),
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
    for ((mut row, alive), screen) in rows.into_iter().zip(alive).zip(screens) {
        let (screen, detail, prompt) = match screen {
            Some((screen, detail, prompt)) => (Some(screen), detail, prompt),
            None => (None, None, None),
        };
        let (state, write) = swarm::screen::resolve(
            row.state.as_deref(),
            row.state_at,
            row.state_source.as_deref(),
            screen,
            now,
        );
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
                ),
            ) {
                Ok(true) => {
                    (row.state_at, row.state_source, row.state_detail) =
                        (Some(now), Some("screen".into()), detail);
                }
                // A newer hook report landed after this listing read the row; it stands,
                // and the next listing shows it.
                Ok(false) => {}
                Err(error) => eprintln!("swarm: {error}"),
            }
        }
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
            log: row.log.map(resolved_agent_log),
            prompt,
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
    for (index, session) in sessions.iter().enumerate() {
        let adapter = session.adapter.as_deref().expect("filtered adapter").trim();
        let listing = swarm::adapter::load(root, adapter).and_then(|mut adapter| {
            adapter.session_id = Some(session.id.clone());
            let now = std::time::Instant::now();
            adapter.deadline = Some(
                now + deadline.saturating_duration_since(now) / (sessions.len() - index) as u32,
            );
            list_agents(connection, root, &session.id, &adapter)
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

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // The commit this binary was built from, which is the only way a machine can tell the bus it
    // runs from the bus the repository states. `build.rs` stamps it. See `ui/Tools/build.sh`.
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
        && Provider::parse(provider).is_some()
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
        let resolved = resolve_role(role, provider, None, false)?;
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
    if let [cmd, provider_flag, provider, json] = args
        && cmd == "accounts"
        && provider_flag == "--provider"
        && json == "--json"
    {
        return print_json(&load_accounts(provider, true)?);
    }
    if let [cmd, json] = args
        && cmd == "usage"
        && json == "--json"
    {
        let command = yelo_command();
        let json = tool_stdout(&command, &["usage", "show", "--json"])?;
        let accounts = Provider::ALL
            .into_iter()
            .filter(|provider| provider.has_accounts())
            .map(|provider| load_accounts(provider.id(), false))
            .collect::<Result<Vec<_>, _>>()?;
        let (usage, skipped) = swarm::profiles::translate_usage(&json, &accounts)
            .map_err(|error| format!("swarm: {error}"))?;
        for reason in skipped {
            eprintln!("swarm: skipped usage row: {reason}");
        }
        return print_json(&usage);
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
        // A chair in a Herdr or tmux pane is registered here, so it needs no ids of its own. The
        // app's `tmux-solo` chair is launched as an agent instead. A child pane already has an
        // agent id, and registering it would take its pane from its own session.
        if env::var_os("SWARM_AGENT_ID").is_none()
            && matches!(adapter.as_str(), "herdr" | "tmux")
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
        set_chair_for_caller(&connection, &session_id, &agent_id, parse_chair(value)?)?;
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
        && sub == "archive"
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
        swarm::store::archive_sessions(&mut connection, &ids)?;
        return Ok(());
    }
    if let [cmd, json] = args
        && cmd == "sessions"
        && json == "--json"
    {
        let mut sessions = Vec::new();
        for row in swarm::store::sessions(&connection)? {
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
            (None, None, None, None);
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
        let resolved: swarm::bus::ResolvedRole = match requested_model {
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
                serde_json::from_value(serde_json::to_value(runner)?)?
            }
            None => serde_json::from_value(resolve_role(role, requested_provider, account, true)?)
                .map_err(|error| format!("swarm: cannot resolve role {role}: {error}"))?,
        };
        if let Some(reason) = swarm::bus::fable_refusal(agent_id, role, resolved.model.as_deref()) {
            return Err(reason.into());
        }
        let provider = resolved.provider.clone();
        let mut command = swarm::bus::argv(agent_id, role, &resolved, &swarm::paths::home()?)?;
        let kind = Provider::parse(provider.as_deref().unwrap_or_default())
            .ok_or("swarm: launch has no known provider")?;
        let mut extra = swarm::bus::extra_args(kind, extra)?;
        // yelo's pick for `auto` can change between two calls, so the trust entry and the pane
        // both use this one answer.
        // A provider with no account source launches on its CLI's own login, so `auto` means
        // nothing there; the chat profile passes it whichever runner starts.
        let picked = match (account, kind.has_accounts()) {
            (Some(requested), true) => match load_accounts(kind.id(), true)
                .map_err(|error| error.to_string().trim_start_matches("swarm: ").to_string())
                .and_then(|accounts| {
                    swarm::profiles::resolve_account(&accounts, requested).cloned()
                }) {
                Ok(account) => Some(account),
                // `auto` is a preference. With no yelo or no automatic account the CLI's own
                // login runs, as the probe already counts a missing yelo as can run (ADR 0032).
                Err(error) if requested == "auto" => {
                    eprintln!("swarm: {error}; {} uses its own login", kind.id());
                    None
                }
                Err(error) => return Err(format!("swarm: {error}").into()),
            },
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
            required: picked.is_some(),
            cwd: &cwd,
        };
        match kind {
            Provider::Codex | Provider::Agy => match trust_target(&cwd, &user_home) {
                Ok(target) if kind == Provider::Codex => {
                    // Without --account, yelo's `codex` in the pane picks the profile, so every
                    // profile it might pick needs the entry.
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
                    // Claude reads `.claude.json` from its CLAUDE_CONFIG_DIR. Without --account,
                    // yelo's `claude` in the pane points that at the profile it picks, and a pane
                    // with no yelo reads ~/.claude.json, so each of them needs the entry.
                    let configs = if let Some(account) = &picked {
                        vec![std::path::PathBuf::from(&account.home).join(".claude.json")]
                    } else {
                        claude_configs(&user_home)
                    };
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
                command: &command,
            },
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
        return spawn_agent(&connection, &root, agent_id, role, options);
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
        let pane = swarm::store::pane_of(&connection, &session_id, child)?
            .ok_or("swarm: no pane recorded")?;
        swarm::adapter::load(&root, &adapter_name())?.run("close", &[("pane", &pane)])?;
        return swarm::store::clear_pane(&connection, &session_id, child);
    }
    if let [cmd, agent_id] = args
        && cmd == "type"
    {
        let text = std::io::read_to_string(std::io::stdin())?;
        if text.trim().is_empty() {
            return Err("swarm: empty text".into());
        }
        let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        if !adapter.has_pane(&pane)? {
            return Err(
                "swarm: this chat's pane has closed; start a new chat or switch model".into(),
            );
        }
        adapter.run("ring", &[("pane", &pane), ("text", &text)])?;
        return Ok(());
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
        let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
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
        let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
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
        let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
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
            let pane = swarm::store::pane_of(&connection, &session_id, &agent_id)?
                .ok_or("swarm: no pane recorded")?;
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
            })
        };
        assert_eq!(
            hook_report(&claude, waiting, coder).unwrap(),
            report(Some(("waiting", None)), None)
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
            report(Some(("failed", Some("quota".to_string()))), None)
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
            (Some(("waiting", None)), Some(log.clone()))
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
        swarm::store::set_state(&connection, &session, CODER, "done", "hook", None, now).unwrap();
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
        swarm::store::set_state(&connection, &session, CODER, "done", "hook", None, now).unwrap();
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
        swarm::store::set_state(&connection, &session, CODER, "done", "hook", None, now).unwrap();
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
        swarm::store::set_state(&connection, &session, CODER, "done", "hook", None, now).unwrap();
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
        swarm::store::set_state(&connection, &session, CODER, "done", "hook", None, now).unwrap();
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
            register_spawned_pane(&connection, &failed, &session, CODER, "coder", None, &[])
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
            register_spawned_pane(&connection, &working, &session, CODER, "coder", None, &[])
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
