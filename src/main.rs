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

const USAGE: &str = "usage: swarm --version | init | hooks status --json | hooks setup [--plan [--json] | --digest <digest>] | adapter check <name> | session new <talk_mode> [--chair <claude|codex>:<id>] (cwd: pwd -P) | session chair <claude|codex>:<id> | session continue <new_id> <old_id> | session archive <id>... | sessions --json | agent add <agent_id> <role> | herdr-split | host-context --provider <claude|codex|agy> | hook <claude|codex|agy> [event] | roles --json | roles get <role> [--provider <claude|codex|agy>] | roles check --json | roles save --revision <revision> <profile-json> | providers --json | models --provider <claude|codex|agy> --json | accounts --provider <claude|codex|agy> --json | usage --json | agents --json [--all] | messages --json [--after <seq>] | launch <agent_id> <role> [--provider <claude|codex|agy>] [--model <model> for chat] [--account <auto|name>] [--cwd <dir>] [-- <provider args>...] | spawn <agent_id> <role> [--provider <p>] [--account <auto|name>] [-- <cmd>...] | type <agent_id> | answer <agent_id> <prompt_id> <choice> | interrupt <agent_id> | key <agent_id> <Up|C-u> | attach <agent_id> | close <agent_id> | send <recipient> <kind> | finish | exited | sweep [--every <secs>] | drain | inbox | ack <seq>";

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
/// after the plan (ADR 0036). Claude needs no step, because `swarm launch` passes its hooks with
/// `--settings`.
fn hooks(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let user_home = std::path::PathBuf::from(env_var("HOME")?);
    let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
    let homes = codex_homes(&user_home)?;
    let agy_hooks = user_home.join(".gemini/config/hooks.json");
    // A file that swarm cannot read or edit is a conflict in the plan, not an error.
    let plan = || -> Vec<swarm::bus::HookFilePlan> {
        let unreadable = swarm::bus::HookFilePlan::unreadable;
        let mut plans: Vec<_> = homes
            .iter()
            .map(|home| {
                swarm::bus::codex_hook_plan(home, &codex)
                    .unwrap_or_else(|error| unreadable(home.join("config.toml"), error))
            })
            .collect();
        plans.push(
            swarm::bus::agy_hook_plan(&agy_hooks)
                .unwrap_or_else(|error| unreadable(agy_hooks.clone(), error)),
        );
        plans
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["status", "--json"] => print_json(&serde_json::json!({
            "codex": homes.iter().all(|home| swarm::bus::codex_hooks_trusted(home, &codex)),
            "agy": swarm::bus::agy_hooks_set(&agy_hooks),
        })),
        ["setup", "--plan"] => {
            print!("{}", hook_plan_text(&plan()));
            Ok(())
        }
        ["setup", "--plan", "--json"] => print_json(&hook_plan_json(&plan())),
        ["setup", rest @ ..] if matches!(rest, [] | ["--digest", _]) => {
            let lock = swarm::paths::root_dir()?.join("trust.lock");
            swarm::bus::with_lock(&lock, || {
                let plans = plan();
                if let ["--digest", digest] = rest
                    && swarm::bus::hook_plan_digest(&plans) != *digest
                {
                    return Err(
                        "swarm: a hook file changed after the plan; check the plan again".into(),
                    );
                }
                if let Some(conflicts) = hook_conflicts_text(&plans) {
                    return Err(conflicts);
                }
                for plan in &plans {
                    if plan.apply()? {
                        println!("swarm: set up swarm's hooks in {}", plan.path.display());
                    }
                }
                Ok(())
            })?;
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// Folder trust in each Codex home or Claude config a pane may read. With no account picked, one
/// that swarm cannot edit is named and skipped, so one broken spare profile does not stop every
/// launch; the one of a picked account must take the entry.
fn trust_each(
    paths: &[std::path::PathBuf],
    picked: bool,
    ensure: impl Fn(&std::path::Path) -> Result<(), String>,
) -> Result<(), String> {
    for path in paths {
        match ensure(path) {
            Err(error) if !picked => eprintln!("swarm: skipped {}: {error}", path.display()),
            result => result?,
        }
    }
    Ok(())
}

fn hook_diff(plan: &swarm::bus::HookFilePlan) -> String {
    swarm::diff::unified(&plan.path.to_string_lossy(), &plan.before, &plan.after)
}

/// Each conflict with its fix, and a last line that says no file was written; None without one.
fn hook_conflicts_text(plans: &[swarm::bus::HookFilePlan]) -> Option<String> {
    let conflicts: Vec<_> = plans.iter().flat_map(|plan| &plan.conflicts).collect();
    if conflicts.is_empty() {
        return None;
    }
    let mut text: String = conflicts
        .iter()
        .map(|conflict| {
            format!(
                "conflict: {} {}\n  found:  {}\n  wanted: {}\n  fix:    {}\n\n",
                conflict.file, conflict.entry, conflict.found, conflict.wanted, conflict.fix
            )
        })
        .collect();
    let plural = if conflicts.len() == 1 { "" } else { "s" };
    text += &format!("{} conflict{plural}. No file written.", conflicts.len());
    Some(text)
}

fn hook_plan_text(plans: &[swarm::bus::HookFilePlan]) -> String {
    let diffs: String = plans.iter().map(hook_diff).collect();
    let last = match hook_conflicts_text(plans) {
        Some(conflicts) => format!("\n{conflicts}"),
        None if diffs.is_empty() => "swarm's hooks are already set up. No file changes.".into(),
        None => "\nPlan only. No file written. Run `swarm hooks setup` to apply.".into(),
    };
    format!("{diffs}{last}\n")
}

/// The plan for the app: the digest that apply checks, each file that changes with its diff, and
/// each conflict.
fn hook_plan_json(plans: &[swarm::bus::HookFilePlan]) -> serde_json::Value {
    serde_json::json!({
        "digest": swarm::bus::hook_plan_digest(plans),
        "files": plans
            .iter()
            .filter(|plan| plan.after != plan.before)
            .map(|plan| serde_json::json!({"path": plan.path.to_string_lossy(), "diff": hook_diff(plan)}))
            .collect::<Vec<_>>(),
        "conflicts": plans.iter().flat_map(|plan| &plan.conflicts).collect::<Vec<_>>(),
    })
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
    let agents = swarm::store::agents(connection, session_id)?;
    let agent = |id: &str| {
        agents
            .iter()
            .find(|agent| agent.id == id)
            .ok_or_else(|| format!("swarm: {id} is not an agent in session {session_id}"))
    };
    agent(sender)?;
    let (recipient, kind) = swarm::store::route(connection, session_id, sender, recipient, kind)?;
    let pane = agent(&recipient)?
        .pane
        .clone()
        .ok_or_else(|| format!("swarm: {recipient} has no pane; nothing would ring it"))?;
    let seq = swarm::store::send_message(
        connection, root, session_id, sender, &recipient, &kind, body,
    )?;
    if !swarm::store::has_rung_unread(connection, session_id, &recipient)? {
        connection.execute(
            "UPDATE message SET rung_at = unixepoch(), rings = 1 WHERE session_id = ?1 AND seq = ?2",
            (session_id, seq),
        )?;
        let ring = swarm::adapter::load(root, adapter_name)
            .and_then(|a| a.run("ring", &[("pane", &pane), ("text", &ring_text(root))]));
        if let Err(error) = ring {
            eprintln!("swarm: ring failed: {error}");
        }
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
        connection.execute(
            "UPDATE message SET rung_at = unixepoch(), rings = 1
                 WHERE session_id = ?1 AND recipient_id = ?2 AND rings = 0
                   AND NOT EXISTS (
                       SELECT 1 FROM read_mark
                       WHERE read_mark.session_id = message.session_id
                         AND message_seq = message.seq AND agent_id = ?2
                   )",
            (session_id, agent_id),
        )?;
        let ring = swarm::adapter::load(root, adapter_name)
            .and_then(|a| a.run("ring", &[("pane", &pane), ("text", &ring_text(root))]));
        if let Err(error) = ring {
            eprintln!("swarm: ring failed: {error}");
        }
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

/// Ring `agent` again when its unseen messages are due for a second ring.
fn rering_if_due(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent: &str,
    pane: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !swarm::store::rering_due(connection, session_id, agent, RERING_UNSEEN_AFTER_SECS)? {
        return Ok(());
    }
    connection.execute(
        "UPDATE message SET rung_at = unixepoch(), rings = rings + 1
         WHERE session_id = ?1 AND recipient_id = ?2 AND seen_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM read_mark
                           WHERE read_mark.session_id = message.session_id
                             AND message_seq = message.seq AND agent_id = ?2)",
        (session_id, agent),
    )?;
    match adapter.run("ring", &[("pane", pane), ("text", &ring_text(root))]) {
        Ok(_) => eprintln!("swarm: re-ringed {agent}"),
        Err(error) => eprintln!("swarm: re-ring failed for {agent}: {error}"),
    }
    Ok(())
}

/// One sweep pass: re-ring unseen messages, the sweeper's own included, and report each child
/// whose pane is gone.
fn sweep_once(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // A child's ring to the chair can be lost too, and no other sweeper covers the chair.
    if let Some(pane) = swarm::store::pane_of(connection, session_id, agent_id)? {
        rering_if_due(connection, root, adapter, session_id, agent_id, &pane)?;
    }
    for (child, pane) in swarm::store::live_children(connection, session_id, agent_id)? {
        if adapter.has_pane(&pane)? {
            rering_if_due(connection, root, adapter, session_id, &child, &pane)?;
            continue;
        }
        let note = format!("agent {child} died without a summary");
        report_dead(connection, root, &adapter.name, session_id, &child, &note)?;
        println!("dead {child}");
    }
    Ok(())
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
                let target = match (alive, row.pane.as_deref()) {
                    (Some(true), Some(pane)) => Some((pane, row.provider.as_deref())),
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
        if screen == Some(swarm::screen::ScreenState::Idle)
            && prompt.is_none()
            && !matches!(state.as_deref(), Some("working" | "waiting"))
            && let Some(pane) = row.pane.as_deref()
            && let Err(error) = rering_if_due(connection, root, adapter, session_id, &row.id, pane)
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
    return Ok(AgentListOutput {
        agents,
        attachable: adapter.attach.is_some(),
    });
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
        let listing = swarm::adapter::load(&root, adapter).and_then(|mut adapter| {
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
    if args.first().map(String::as_str) == Some("hooks") {
        return hooks(&args[1..]);
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
        let lock = root.join("trust.lock");
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
                    swarm::bus::with_lock(&lock, || {
                        trust_each(&homes, picked.is_some(), |home| {
                            swarm::bus::ensure_codex_trust(home, &target)
                        })
                    })?;
                }
                Ok(target) => {
                    let settings = user_home.join(".gemini/antigravity-cli/settings.json");
                    swarm::bus::with_lock(&lock, || {
                        swarm::bus::ensure_agy_trust(&settings, &target)
                    })?;
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
                // one, so it gets the entry only where Codex and AGY may be pre-trusted.
                let refused = if agent_id == "orchestrator" {
                    trust_target(&cwd, &user_home).err()
                } else {
                    None
                };
                if let Some(reason) = refused {
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
                        let mut configs = vec![user_home.join(".claude.json")];
                        if let Ok(profiles) = std::fs::read_dir(user_home.join(".claude/.profiles"))
                        {
                            configs.extend(
                                profiles
                                    .filter_map(Result::ok)
                                    // yelo keeps its own data in hidden dirs here; a profile is
                                    // not hidden.
                                    .filter(|entry| {
                                        !entry.file_name().to_string_lossy().starts_with('.')
                                            && entry.path().is_dir()
                                    })
                                    .map(|entry| entry.path().join(".claude.json")),
                            );
                        }
                        configs
                    };
                    swarm::bus::with_lock(&lock, || {
                        trust_each(&configs, picked.is_some(), |config| {
                            swarm::bus::ensure_claude_trust(config, &pane_dir).map(|_| ())
                        })
                    })?;
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
                match sweep_once(&mut connection, &root, &adapter, &session_id, &agent_id) {
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

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
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

        let codex = |home: &std::path::Path| swarm::bus::ensure_codex_trust(home, project);
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
        let claude =
            |config: &std::path::Path| swarm::bus::ensure_claude_trust(config, project).map(|_| ());

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
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);
        connection
            .execute("UPDATE message SET rung_at = unixepoch() - 61", [])
            .unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));

        swarm::store::inbox(&connection, &session, CODER).unwrap();
        connection
            .execute("UPDATE message SET rung_at = unixepoch() - 61", [])
            .unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        swarm::store::ack(&connection, &session, 1, CODER).unwrap();
        connection
            .execute(
                "UPDATE message SET seen_at = NULL, rung_at = unixepoch() - 61 WHERE seq = 1",
                [],
            )
            .unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();

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

        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();

        assert_eq!(
            swarm::store::pane_of(&connection, &session, CODER).unwrap(),
            None
        );
        assert!(swarm::store::has_summary(&connection, &session, CODER).unwrap());
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "%1\n");
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

        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();

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
        swarm::bus::ensure_codex_trust(&codex, std::path::Path::new("/project")).unwrap();
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
