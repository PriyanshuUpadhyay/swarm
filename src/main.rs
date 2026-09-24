use std::env;
use std::os::unix::process::ExitStatusExt;

const RERING_UNSEEN_AFTER_SECS: i64 = 60;

/// The ring is typed into the recipient's prompt, so it names the next step. An agent that did
/// not load the swarm skill otherwise takes the bare ring as the whole task and waits.
fn ring_text(root: &std::path::Path) -> String {
    format!("swarm: new message. Run swarm inbox, read each body at {}/<body_path>, then swarm ack <seq>.", root.display())
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

    Ok(())
}

const USAGE: &str = "usage: swarm --version | init | adapter check <name> | session new <talk_mode> [--chair <claude|codex>:<id>] (cwd: pwd -P) | session chair <claude|codex>:<id> | session continue <new_id> <old_id> | session archive <id>... | sessions --json | agent add <agent_id> <role> | roles --json | roles set-model <runner> <model> | accounts --provider <claude|codex|agy> --json | usage --json | agents --json | messages --json [--after <seq>] | launch <agent_id> <role> [--provider <claude|codex|agy>] [--account <auto|name>] [--cwd <dir>] [-- <provider args>...] | spawn <agent_id> <role> [--provider <p>] [--account <auto|name>] [-- <cmd>...] | type <agent_id> | interrupt <agent_id> | attach <agent_id> | close <agent_id> | send <recipient> <kind> | finish | exited | sweep [--every <secs>] | drain | inbox | ack <seq>";

fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("swarm: {name} not set"))
}

fn session_id() -> Result<String, String> {
    let id = env_var("SWARM_SESSION_ID")?;
    let id = uuid::Uuid::parse_str(&id).map_err(|_| "swarm: bad SWARM_SESSION_ID".to_string())?;
    if id.get_version_num() != 7 {
        return Err("swarm: bad SWARM_SESSION_ID".to_string());
    }
    Ok(id.to_string())
}

fn adapter_name() -> String {
    env::var("SWARM_ADAPTER").unwrap_or("tmux".into())
}

fn identity() -> Result<(String, String), String> {
    Ok((session_id()?, env_var("SWARM_AGENT_ID")?))
}

fn run_tool(executable: &str, args: &[&str]) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    std::process::Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| format!("swarm: cannot run {executable}: {error}").into())
}

fn tool_stdout(executable: &str, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = run_tool(executable, args)?;
    if !output.status.success() {
        return Err(format!("swarm: {executable} failed: {}", String::from_utf8_lossy(&output.stderr).trim()).into());
    }
    String::from_utf8(output.stdout).map_err(|error| format!("swarm: {executable} printed non-UTF-8 output: {error}").into())
}

fn routing_command() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(command) = env::var("SWARM_ROUTING_CMD") {
        return Ok(command);
    }
    Ok(std::path::Path::new(&env_var("HOME")?)
        .join(".claude/scripts/agent-routing.mjs")
        .to_string_lossy()
        .into_owned())
}

fn yelo_command() -> String {
    env::var("SWARM_YELO_CMD").unwrap_or_else(|_| "yelo".to_string())
}

fn load_roles() -> Result<swarm::profiles::RoleList, Box<dyn std::error::Error>> {
    let command = routing_command()?;
    let json = tool_stdout(&command, &["web-state"])?;
    swarm::profiles::translate_roles(&json).map_err(|error| format!("swarm: {error}").into())
}

fn set_role_model(command: &str, runner: &str, model: &str) -> Result<String, Box<dyn std::error::Error>> {
    if model.is_empty() || model.chars().any(char::is_whitespace) {
        return Err("swarm: model must be one non-empty name".into());
    }
    tool_stdout(command, &["bump", runner, model])
}

fn resolve_role(role: &str, provider: Option<&str>) -> Result<swarm::bus::ResolvedRole, Box<dyn std::error::Error>> {
    let command = routing_command()?;
    let mut args = vec!["get", role];
    if let Some(provider) = provider { args.extend(["--provider", provider]); }
    let output = run_tool(&command, &args)?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr).trim().replace(['\r', '\n'], " ");
        return Err(format!("swarm: cannot resolve role {role}: {reason}").into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("swarm: cannot resolve role {role}: {error}"))?;
    if let Some(error) = value.get("error") {
        let reason = error.as_str().map(str::to_string).unwrap_or_else(|| error.to_string());
        return Err(format!("swarm: cannot resolve role {role}: {reason}").into());
    }
    serde_json::from_value(value).map_err(|error| format!("swarm: cannot resolve role {role}: {error}").into())
}

fn load_accounts(provider: &str, with_pick: bool) -> Result<swarm::profiles::AccountList, Box<dyn std::error::Error>> {
    if provider == "agy" {
        return Ok(swarm::profiles::empty_accounts(provider));
    }
    if !matches!(provider, "claude" | "codex") {
        return Err(format!("swarm: unknown provider {provider}").into());
    }
    let command = yelo_command();
    let list = tool_stdout(&command, &["profile", "list", "--cli", provider, "--usage", "--json"])?;
    let pick_json = if with_pick {
        let pick = run_tool(&command, &["profile", "pick", "--cli", provider, "--json"])?;
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
    for (provider, variable) in [("claude", "CLAUDE_CODE_SESSION_ID"), ("codex", "CODEX_THREAD_ID")] {
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

fn claude_chair_log(id: &str) -> Option<std::path::PathBuf> {
    let config = env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".claude")))?;
    std::fs::read_dir(config.join("projects"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(format!("{id}.jsonl")))
        .find(|path| path.is_file())
}

fn codex_chair_log(id: &str, days: &[String; 3]) -> Option<std::path::PathBuf> {
    let home = env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".codex")))?;
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
                return Ok(SpawnOptions { provider, account, command: &args[index + 1..] });
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
    Ok(SpawnOptions { provider, account, command: &[] })
}

/// Refuse a recipient without a pane, then store the message and ring it. The bell is a hint
/// (R9), so a ring failure only warns.
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
    let (recipient, kind) = swarm::store::route(connection, session_id, sender, recipient, kind)?;
    let pane = swarm::store::pane_of(connection, session_id, &recipient)?
        .ok_or_else(|| format!("swarm: {recipient} has no pane; nothing would ring it"))?;
    let seq = swarm::store::send_message(connection, root, session_id, sender, &recipient, &kind, body)?;
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
        && swarm::store::has_unrung_unread(connection, session_id, agent_id)? {
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

/// What the provider CLI on PATH knows as models, for `swarm::bus::model_known`. None when the
/// check cannot run; agy is skipped because `agy models` asks the network (about 5 s).
fn model_catalog(provider: &str, account_env: &std::collections::BTreeMap<String, String>) -> Option<Vec<u8>> {
    match provider {
        "claude" => env::split_paths(&env::var_os("PATH")?)
            .map(|dir| dir.join("claude"))
            .find(|path| path.is_file())
            .and_then(|path| std::fs::read(path).ok()),
        "codex" => std::process::Command::new("codex")
            .args(["debug", "models"])
            .envs(account_env)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| output.stdout),
        _ => None,
    }
}

/// Where `launch` may pre-trust Codex and AGY for `cwd`; see `swarm::bus::trust_target`.
fn trust_target(cwd: &std::path::Path, user_home: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let git_root = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| std::fs::canonicalize(String::from_utf8_lossy(&output.stdout).trim()).ok());
    let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(user_home).map_err(|error| error.to_string())?);
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
    swarm::bus::trust_target(cwd, git_root.as_deref(), &std::fs::canonicalize(user_home).map_err(|error| error.to_string())?, &scratch_roots)
}

fn spawn_agent(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    agent_id: &str,
    role: &str,
    options: SpawnOptions<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let account = if let Some(requested) = options.account {
        let provider = match options.provider {
            Some(provider) => provider.to_string(),
            None => load_roles()?
                .roles
                .into_iter()
                .find(|entry| entry.role == *role)
                .map(|entry| entry.provider)
                .ok_or_else(|| format!("swarm: unknown role {role}"))?,
        };
        let accounts = load_accounts(&provider, true)?;
        Some(swarm::profiles::resolve_account(&accounts, requested).map_err(|error| format!("swarm: {error}"))?.clone())
    } else {
        None
    };
    let session_id = session_id()?;
    let provider = options.provider.or_else(|| options.command.first().map(String::as_str))
        .filter(|provider| matches!(*provider, "claude" | "codex" | "agy"))
        .map(str::to_string);
    // Warn, do not refuse: an unknown name usually means the role config or the CLI is stale,
    // and the pane shows the real error if the model does not run.
    if let (Some(provider), Some(model)) = (provider.as_deref(), swarm::bus::command_model(options.command)) {
        let account_env = account.as_ref().map(|account| account.env.clone()).unwrap_or_default();
        if model_catalog(provider, &account_env).is_some_and(|catalog| !swarm::bus::model_known(provider, &catalog, model)) {
            eprintln!("swarm: warning: the installed {provider} CLI does not list model {model}; the role config may need an update");
        }
    }
    let adapter = swarm::adapter::load(root, &adapter_name())?;
    let session = session_id.to_string();
    let home = swarm::paths::home()?;
    let current_adapter = adapter_name();
    let vars = [("session_id", session.as_str()), ("agent_id", agent_id), ("home", home.as_str()), ("adapter", current_adapter.as_str())];
    let pane = register_spawned_pane(
        connection,
        &adapter,
        &session_id,
        agent_id,
        role,
        provider.as_deref(),
        &vars,
    )?;
    if !options.command.is_empty() {
        let exe = env::current_exe()?.to_string_lossy().into_owned();
        let hook = swarm::adapter::shell_line(&[exe, "exited".into()]);
        let child = if let Some(account) = &account {
            let mut args = vec!["env".to_string(), "--".to_string()];
            args.extend(account.env.iter().map(|(key, value)| format!("{key}={value}")));
            args.extend_from_slice(options.command);
            swarm::adapter::shell_line(&args)
        } else {
            swarm::adapter::shell_line(options.command)
        };
        adapter.run("ring", &[("pane", &pane), ("text", &format!("{child}; {hook}"))])?;
    }
    println!("{pane}");
    if let Some(account) = account {
        eprintln!("account {}", account.name);
    }
    Ok(())
}

fn attach(agent_id: &str) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?.ok_or("swarm: no pane recorded")?;
    swarm::adapter::load(&root, &adapter_name())?.attach(&[("pane", &pane)])
}

/// A child ended without `swarm finish`: forget its pane, then send a fallback summary in its
/// name and queue a summarize job unless it already sent one (R19).
fn report_dead(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session_id: &str,
    child: &str,
    note: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    swarm::store::clear_pane(connection, session_id, child)?;
    if !swarm::store::has_summary(connection, session_id, child)? {
        let orchestrator = swarm::store::orchestrator_of(connection, session_id)?;
        deliver(connection, root, &adapter_name(), session_id, child, &orchestrator, "summary", note)?;
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
        report_dead(connection, root, session_id, &child, &note)?;
        println!("dead {child}");
    }
    Ok(())
}

/// Feed the agent's captured log to the summarizer shell command and return its output.
fn summarize_log(log: &std::path::Path, summarizer: &str) -> Result<String, Box<dyn std::error::Error>> {
    let input = std::fs::File::open(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let output = std::process::Command::new("sh").arg("-c").arg(summarizer).stdin(input).output()?;
    if !output.status.success() {
        return Err(format!("summarizer failed: {}", String::from_utf8_lossy(&output.stderr).trim()).into());
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
        deliver(connection, root, adapter_name, &session, &agent, &orchestrator, "summary", &summary)?;
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

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // The commit this binary was built from, which is the only way a machine can tell the bus it
    // runs from the bus the repository states. `build.rs` stamps it. See `ui/Tools/build.sh`.
    if let [flag] = args && (flag == "--version" || flag == "-V") {
        println!("swarm {} {}", env!("CARGO_PKG_VERSION"), env!("SWARM_BUILD_COMMIT"));
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("init") {
        return init();
    }
    if let [cmd, json] = args && cmd == "roles" && json == "--json" {
        return print_json(&load_roles()?);
    }
    if let [cmd, sub, runner, model] = args && cmd == "roles" && sub == "set-model" {
        let output = set_role_model(&routing_command()?, runner, model)?;
        print!("{output}");
        return Ok(());
    }
    if let [cmd, provider_flag, provider, json] = args
        && cmd == "accounts"
        && provider_flag == "--provider"
        && json == "--json"
    {
        return print_json(&load_accounts(provider, true)?);
    }
    if let [cmd, json] = args && cmd == "usage" && json == "--json" {
        let command = yelo_command();
        let json = tool_stdout(&command, &["usage", "show", "--json"])?;
        let accounts = [load_accounts("claude", false)?, load_accounts("codex", false)?];
        let (usage, skipped) =
            swarm::profiles::translate_usage(&json, &accounts).map_err(|error| format!("swarm: {error}"))?;
        for reason in skipped {
            eprintln!("swarm: skipped usage row: {reason}");
        }
        return print_json(&usage);
    }
    let root = swarm::paths::root_dir()?;
    if let [cmd, sub, name] = args && cmd == "adapter" && sub == "check" {
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
        println!(
            "{}",
            swarm::store::create_session(
                &connection,
                talk_mode,
                &env::current_dir()?,
                chair.as_ref().map(|(provider, id)| (provider.as_str(), id.as_str())),
                Some(&adapter),
            )?
        );
        return Ok(());
    }
    if let [cmd, sub, value] = args && cmd == "session" && sub == "chair" {
        let (session_id, agent_id) = identity()?;
        set_chair_for_caller(&connection, &session_id, &agent_id, parse_chair(value)?)?;
        return Ok(());
    }
    if let [cmd, sub, new_id, old_id] = args && cmd == "session" && sub == "continue" {
        return swarm::store::continue_session(&connection, new_id, old_id);
    }
    if let [cmd, sub, ids @ ..] = args
        && cmd == "session"
        && sub == "archive"
        && !ids.is_empty()
    {
        let ids = ids.iter().map(|id| {
            let id = uuid::Uuid::parse_str(id).map_err(|_| USAGE)?;
            if id.get_version_num() != 7 { return Err(USAGE); }
            Ok(id.to_string())
        }).collect::<Result<Vec<_>, &str>>()?;
        swarm::store::archive_sessions(&mut connection, &ids)?;
        return Ok(());
    }
    if let [cmd, json] = args && cmd == "sessions" && json == "--json" {
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
    if let [cmd, sub, agent_id, role] = args && cmd == "agent" && sub == "add" {
        return add_agent(&connection, &root, &adapter_name(), &session_id()?, agent_id, role);
    }
    if let [cmd, json] = args && cmd == "agents" && json == "--json" {
        let session_id = session_id()?;
        let rows = swarm::store::agents(&connection, &session_id)?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        let listing = match adapter.run("list", &[]) {
            Ok(listing) => Some(listing),
            Err(error) => {
                eprintln!("swarm: {}", error.to_string().replace(['\r', '\n'], " "));
                None
            }
        };
        let agents = rows
            .into_iter()
            .map(|row| {
                let alive = row
                    .pane
                    .as_deref()
                    .and_then(|pane| listing.as_deref().map(|list| swarm::adapter::listing_has_pane(list, pane)));
                swarm::bus::Agent { id: row.id, role: row.role, pane: row.pane, provider: row.provider, created_at: row.created_at, alive }
            })
            .collect();
        #[derive(serde::Serialize)]
        struct AgentListOutput {
            agents: Vec<swarm::bus::Agent>,
            attachable: bool,
        }
        return print_json(&AgentListOutput {
            agents,
            attachable: adapter.attach.is_some(),
        });
    }
    if let [cmd, rest @ ..] = args && cmd == "messages" {
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
    if let [cmd, agent_id, role, rest @ ..] = args && cmd == "launch" {
        if !swarm::bus::valid_agent_id(agent_id) {
            return Err(format!("swarm: bad agent id {agent_id}").into());
        }
        let herdr_agent_pane = env::var("HERDR_AGENT_PANE").ok();
        if let Some(reason) = swarm::bus::launch_refusal(env::var("SWARM_AGENT_ID").ok().as_deref(), herdr_agent_pane.as_deref()) {
            return Err(reason.into());
        }
        // Fail before any trust write, so a stray launch outside a session changes nothing.
        session_id()?;
        let (options, extra) = match rest.iter().position(|arg| arg == "--") {
            Some(index) => (&rest[..index], &rest[index + 1..]),
            None => (rest, &[][..]),
        };
        let (mut account, mut cwd, mut requested_provider) = (None, None, None);
        for pair in options.chunks(2) {
            match pair {
                [flag, value] if flag == "--account" => account = Some(value.as_str()),
                [flag, value] if flag == "--cwd" => cwd = Some(std::path::PathBuf::from(value)),
                [flag, value] if flag == "--provider" && matches!(value.as_str(), "claude" | "codex" | "agy") => requested_provider = Some(value.as_str()),
                _ => return Err(USAGE.into()),
            }
        }
        let cwd = cwd.map_or_else(env::current_dir, Ok)?;
        let cwd = std::fs::canonicalize(&cwd).map_err(|error| format!("swarm: bad --cwd {}: {error}", cwd.display()))?;
        let resolved = resolve_role(role, requested_provider)?;
        if let Some(reason) = swarm::bus::fable_refusal(agent_id, role, resolved.model.as_deref()) {
            return Err(reason.into());
        }
        let provider = resolved.provider.clone();
        let mut command = swarm::bus::argv(agent_id, role, &resolved, &swarm::paths::home()?)?;
        let mut extra = swarm::bus::extra_args(provider.as_deref().unwrap_or_default(), extra)?;
        let mut pane_dir = cwd.clone();
        let user_home = std::path::PathBuf::from(env_var("HOME")?);
        let lock = root.join("trust.lock");
        match provider.as_deref() {
            Some(provider @ ("codex" | "agy")) => match trust_target(&cwd, &user_home) {
                Ok(target) if provider == "codex" => {
                    // Without --account, yelo's `codex` in the pane picks the profile, so every
                    // profile it might pick needs the entry.
                    let homes = if let Some(account_name) = account {
                        let accounts = load_accounts("codex", true)?;
                        let account = swarm::profiles::resolve_account(&accounts, account_name)
                            .map_err(|error| format!("swarm: {error}"))?;
                        vec![std::path::PathBuf::from(&account.home)]
                    } else {
                        let mut homes = vec![default_codex_home()?];
                        homes.extend(
                            std::fs::read_dir(&user_home)?
                                .filter_map(Result::ok)
                                .filter(|entry| entry.file_name().to_string_lossy().starts_with(".codex-") && entry.path().is_dir())
                                .map(|entry| entry.path()),
                        );
                        homes.sort();
                        homes.dedup();
                        homes
                    };
                    swarm::bus::with_lock(&lock, || homes.iter().try_for_each(|home| swarm::bus::ensure_codex_trust(home, &target)))?;
                }
                Ok(target) => {
                    let settings = user_home.join(".gemini/antigravity-cli/settings.json");
                    swarm::bus::with_lock(&lock, || swarm::bus::ensure_agy_trust(&settings, &target))?;
                }
                Err(reason) => eprintln!("swarm: not pre-trusting for {provider}: {reason}; answer the prompt in the pane"),
            },
            // The chair a person starts can answer its own trust dialog in the pane (ADR 0008).
            Some("claude") if agent_id != "orchestrator" => {
                let (dir, args) = swarm::bus::claude_child(agent_id, &cwd, &extra, &uuid::Uuid::now_v7());
                std::fs::create_dir_all(&dir)?;
                pane_dir = std::fs::canonicalize(&dir)?;
                let config = user_home.join(".claude.json");
                swarm::bus::with_lock(&lock, || swarm::bus::ensure_claude_trust(&config, &pane_dir))?;
                extra = args;
            }
            _ => {}
        }
        command.extend(extra);
        // Every adapter's spawn verb opens the pane in the working directory it runs in.
        env::set_current_dir(&pane_dir)?;
        return spawn_agent(
            &connection,
            &root,
            agent_id,
            role,
            SpawnOptions { provider: provider.as_deref(), account, command: &command },
        );
    }
    if let [cmd, agent_id, role, rest @ ..] = args && cmd == "spawn" {
        let options = parse_spawn_options(rest)?;
        return spawn_agent(&connection, &root, agent_id, role, options);
    }
    if let [cmd] = args && cmd == "drain" {
        let summarizer = env_var("SWARM_SUMMARIZER")?;
        while let Some(job_id) = swarm::store::claim_next(&connection)? {
            println!("{}", process_summary_job(&mut connection, &root, &adapter_name(), &summarizer, job_id)?);
        }
        return Ok(());
    }
    if let [cmd, child] = args && cmd == "close" {
        let session_id = session_id()?;
        let pane = swarm::store::pane_of(&connection, &session_id, child)?.ok_or("swarm: no pane recorded")?;
        swarm::adapter::load(&root, &adapter_name())?.run("close", &[("pane", &pane)])?;
        return swarm::store::clear_pane(&connection, &session_id, child);
    }
    if let [cmd, agent_id] = args && cmd == "type" {
        let text = std::io::read_to_string(std::io::stdin())?;
        if text.trim().is_empty() {
            return Err("swarm: empty text".into());
        }
        let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        if !adapter.has_pane(&pane)? {
            return Err("swarm: this chat's pane has closed; start a new chat or switch model".into());
        }
        adapter.run("ring", &[("pane", &pane), ("text", &text)])?;
        return Ok(());
    }
    if let [cmd, agent_id] = args && cmd == "interrupt" {
        let pane = swarm::store::pane_of(&connection, &session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
        swarm::adapter::load(&root, &adapter_name())?.run("interrupt", &[("pane", &pane)])?;
        return Ok(());
    }
    let (session_id, agent_id) = identity()?;
    match args {
        [cmd, recipient, kind] if cmd == "send" => {
            let body = std::io::read_to_string(std::io::stdin())?;
            let seq = deliver(&mut connection, &root, &adapter_name(), &session_id, &agent_id, recipient, kind, &body)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "finish" => {
            let summary = std::io::read_to_string(std::io::stdin())?;
            let orchestrator = swarm::store::orchestrator_of(&connection, &session_id)?;
            let seq = deliver(&mut connection, &root, &adapter_name(), &session_id, &agent_id, &orchestrator, "summary", &summary)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "exited" => {
            let pane = swarm::store::pane_of(&connection, &session_id, &agent_id)?.ok_or("swarm: no pane recorded")?;
            let text = swarm::adapter::load(&root, &adapter_name())?.run("capture", &[("pane", &pane)])?;
            let run_dir = root.join(format!("runs/{session_id}"));
            std::fs::create_dir_all(&run_dir)?;
            swarm::store::write_atomic(&run_dir.join(format!("{agent_id}.log")), &text)?;
            let note = format!("agent {agent_id} exited without a summary");
            report_dead(&mut connection, &root, &session_id, &agent_id, &note)
        }
        [cmd, rest @ ..] if cmd == "sweep" => {
            let every = match rest {
                [] => None,
                [flag, secs] if flag == "--every" => Some(secs.parse::<u64>().ok().filter(|s| *s > 0).ok_or(USAGE)?),
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
            ack(&mut connection, &root, &adapter_name(), &session_id, &agent_id, seq)
        }
        _ => Err(USAGE.into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let [cmd, agent_id] = args.as_slice()
        && cmd == "attach"
    {
        match attach(agent_id) {
            Ok(status) => std::process::exit(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0))),
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

    const ORCHESTRATOR: &str = "orchestrator";
    const CODER: &str = "coder";

    #[test]
    fn model_edit_calls_the_router_and_rejects_blank_names() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!("swarm-model-test-{}", uuid::Uuid::now_v7()));
        std::fs::write(&path, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();

        let command = path.to_str().unwrap();
        assert_eq!(
            set_role_model(command, "codex-sol-high-agent", "gpt-6-sol").unwrap(),
            "bump\ncodex-sol-high-agent\ngpt-6-sol\n"
        );
        assert!(set_role_model(command, "codex-sol-high-agent", " ").is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn sweep_rerings_a_child_once_for_old_unseen_messages() {
        let root = std::env::temp_dir().join(format!("swarm-sweep-test-{}", std::process::id()));
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        swarm::store::send_message(&mut connection, &root, &session, ORCHESTRATOR, CODER, "ask", "one").unwrap();
        swarm::store::send_message(&mut connection, &root, &session, ORCHESTRATOR, CODER, "ask", "two").unwrap();
        connection.execute("UPDATE message SET created_at = unixepoch() - 61", []).unwrap();

        let adapter = swarm::adapter::parse(
            "fake",
            &format!("self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = echo %2\nclose = true\ncapture = true\n", ring_log.display()),
        )
        .unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);
        connection.execute("UPDATE message SET rung_at = unixepoch() - 61", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));

        swarm::store::inbox(&connection, &session, CODER).unwrap();
        connection.execute("UPDATE message SET rung_at = unixepoch() - 61", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();
        swarm::store::ack(&connection, &session, 1, CODER).unwrap();
        connection.execute("UPDATE message SET seen_at = NULL, rung_at = unixepoch() - 61 WHERE seq = 1", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, &session, ORCHESTRATOR).unwrap();

        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), ring.repeat(2));
    }

    #[test]
    fn sweep_reports_a_dead_child_as_before() {
        let root = std::env::temp_dir().join(format!("swarm-dead-test-{}", std::process::id()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
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

        assert_eq!(swarm::store::pane_of(&connection, &session, CODER).unwrap(), None);
        assert!(swarm::store::has_summary(&connection, &session, CODER).unwrap());
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
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();

        let first = deliver(&mut connection, &root, "fake", &session, ORCHESTRATOR, CODER, "ask", "first").unwrap();
        let second = deliver(&mut connection, &root, "fake", &session, ORCHESTRATOR, CODER, "ask", "second").unwrap();
        assert_eq!((first, second), (0, 1));
        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);

        ack(&mut connection, &root, "fake", &session, CODER, first).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
        ack(&mut connection, &root, "fake", &session, CODER, second).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
        deliver(&mut connection, &root, "fake", &session, ORCHESTRATOR, CODER, "ask", "third").unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(3));

        // The coder listed "third" but has not acked it, so "fourth" is news and must ring.
        swarm::store::inbox(&connection, &session, CODER).unwrap();
        deliver(&mut connection, &root, "fake", &session, ORCHESTRATOR, CODER, "ask", "fourth").unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(4));
    }

    #[test]
    fn sweep_rerings_its_own_chair_for_an_old_unseen_finish() {
        let root = std::env::temp_dir().join(format!("swarm-sweep-chair-test-{}", std::process::id()));
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
        swarm::store::send_message(&mut connection, &root, &session, CODER, ORCHESTRATOR, "summary", "done").unwrap();
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
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();

        add_agent(&connection, &root, "fake", &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        deliver(&mut connection, &root, "fake", &session, CODER, ORCHESTRATOR, "summary", "done").unwrap();

        assert_eq!(swarm::store::pane_of(&connection, &session, ORCHESTRATOR).unwrap().as_deref(), Some("%9"));
        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), format!("%9:{}\n", ring_text(&root)));
    }

    #[test]
    fn a_routing_role_chair_receives_a_child_finish() {
        let root = std::env::temp_dir().join(format!("swarm-routing-chair-test-{}", std::process::id()));
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
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, Some("fake")).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "code.complex").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%9").unwrap();

        let chair = swarm::store::orchestrator_of(&connection, &session).unwrap();
        deliver(&mut connection, &root, "fake", &session, CODER, &chair, "summary", "done").unwrap();

        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), format!("%9:{}\n", ring_text(&root)));
    }

    #[test]
    fn only_the_orchestrator_can_set_the_session_chair() {
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "code.complex").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();

        let error = set_chair_for_caller(&connection, &session, CODER, Some(("claude", "wrong")))
            .unwrap_err()
            .to_string();
        assert_eq!(error, "swarm: only the orchestrator can set the session chair");
        set_chair_for_caller(&connection, &session, ORCHESTRATOR, Some(("claude", "right"))).unwrap();
        let stored = swarm::store::sessions(&connection).unwrap();
        assert_eq!(stored[0].chair_id.as_deref(), Some("right"));
    }

    #[test]
    fn a_failed_adapter_spawn_rolls_back_the_agent() {
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        let failed = swarm::adapter::parse(
            "failed",
            "self = true\nspawn = echo refused >&2; exit 1\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();

        assert!(register_spawned_pane(&connection, &failed, &session, CODER, "coder", None, &[]).is_err());
        assert!(swarm::store::agents(&connection, &session).unwrap().is_empty());

        let working = swarm::adapter::parse(
            "working",
            "self = true\nspawn = printf '%s' '%2'\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        assert_eq!(
            register_spawned_pane(&connection, &working, &session, CODER, "coder", None, &[]).unwrap(),
            "%2"
        );
    }

    #[test]
    fn a_failed_dead_agent_report_still_clears_its_pane() {
        let root = std::env::temp_dir().join(format!("swarm-dead-report-test-{}", std::process::id()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();

        assert!(report_dead(&mut connection, &root, &session, CODER, "dead").is_err());
        assert_eq!(swarm::store::pane_of(&connection, &session, CODER).unwrap(), None);
    }

    #[test]
    fn a_failed_summary_delivery_keeps_the_log_and_releases_the_job() {
        let root = std::env::temp_dir().join(format!("swarm-summary-retry-test-{}", std::process::id()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
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
        let state: String = connection.query_row("SELECT state FROM job WHERE id = ?1", [job], |row| row.get(0)).unwrap();
        assert_eq!(state, "queued");
    }

    #[test]
    fn default_codex_trust_uses_codex_home_or_login_home() {
        let root = std::env::temp_dir().join(format!("swarm-default-codex-test-{}", std::process::id()));
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
            default_codex_home_with_env(|name| (name == "CODEX_HOME").then(|| override_home.clone().into_os_string())).unwrap(),
            override_home
        );
    }

    #[test]
    fn a_chair_pane_moves_to_the_new_session() {
        let root = std::env::temp_dir().join(format!("swarm-chair-move-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = printf '%s' '%1'\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let first = swarm::store::create_session(&connection, "lane", std::path::Path::new("/first"), None, None).unwrap();
        let second = swarm::store::create_session(&connection, "lane", std::path::Path::new("/second"), None, None).unwrap();

        add_agent(&connection, &root, "fake", &first, ORCHESTRATOR, "orchestrator").unwrap();
        add_agent(&connection, &root, "fake", &second, ORCHESTRATOR, "orchestrator").unwrap();

        assert_eq!(swarm::store::pane_of(&connection, &first, ORCHESTRATOR).unwrap(), None);
        assert_eq!(swarm::store::pane_of(&connection, &second, ORCHESTRATOR).unwrap().as_deref(), Some("%1"));
    }

    #[test]
    fn orchestrator_registration_sets_the_session_adapter() {
        let root = std::env::temp_dir().join(format!("swarm-chair-adapter-test-{}", std::process::id()));
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

        add_agent(&connection, &root, "fake", &session, ORCHESTRATOR, "orchestrator").unwrap();

        let stored = swarm::store::sessions(&connection).unwrap();
        assert_eq!(stored[0].adapter.as_deref(), Some("fake"));
    }

    #[test]
    fn refused_orchestrator_registration_leaves_no_agent() {
        let root = std::env::temp_dir().join(format!("swarm-empty-chair-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();

        let error = add_agent(&connection, &root, "fake", &session, ORCHESTRATOR, "orchestrator")
            .unwrap_err()
            .to_string();

        assert_eq!(error, "swarm: adapter gave no pane for orchestrator");
        assert!(swarm::store::agents(&connection, &session).unwrap().is_empty());
    }

    #[test]
    fn deliver_to_agent_without_pane_writes_no_message() {
        let root = std::env::temp_dir().join(format!("swarm-no-pane-test-{}", std::process::id()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();

        let error = deliver(
            &mut connection, &root, "fake", &session, CODER, ORCHESTRATOR, "summary", "done",
        )
        .unwrap_err()
        .to_string();

        assert_eq!(error, "swarm: orchestrator has no pane; nothing would ring it");
        assert_eq!(swarm::store::messages(&connection, &session, -1).unwrap().len(), 0);
    }
}
