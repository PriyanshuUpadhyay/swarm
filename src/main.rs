use std::env;
use std::os::unix::process::ExitStatusExt;

const RERING_UNSEEN_AFTER_SECS: i64 = 15;

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    let adapters = swarm::paths::root_dir()?.join("adapters");
    std::fs::create_dir_all(&adapters)?;
    let shipped = [("tmux", include_str!("../adapters/tmux.conf")), ("tmux-solo", include_str!("../adapters/tmux-solo.conf")), ("herdr", include_str!("../adapters/herdr.conf"))];
    for (name, text) in shipped {
        let file = adapters.join(format!("{name}.conf"));
        if !file.exists() {
            std::fs::write(file, text)?;
        }
    }
    swarm::store::open(&swarm::paths::sqlite_db()?)?;

    Ok(())
}

const USAGE: &str = "usage: swarm init | adapter check <name> | session new <talk_mode> [--chair <claude|codex>:<id>] | session chair <claude|codex>:<id> | session archive <id>... | sessions --json | agent add <agent_id> <role> | roles --json | accounts --provider <claude|codex|agy> --json | usage --json | agents --json | messages --json [--after <seq>] | launch <agent_id> <role> [--account <auto|name>] | spawn <agent_id> <role> [--provider <p>] [--account <auto|name>] [-- <cmd>...] | type <agent_id> | interrupt <agent_id> | attach <agent_id> | close <agent_id> | send <recipient> <kind> | finish | exited | sweep [--every <secs>] | drain | inbox | ack <seq>";

fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("swarm: {name} not set"))
}

fn session_id() -> Result<i64, String> {
    env_var("SWARM_SESSION_ID")?.parse().map_err(|_| "swarm: bad SWARM_SESSION_ID".to_string())
}

fn adapter_name() -> String {
    env::var("SWARM_ADAPTER").unwrap_or("tmux".into())
}

fn identity() -> Result<(i64, String), String> {
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

fn resolve_role(role: &str) -> Result<swarm::bus::ResolvedRole, Box<dyn std::error::Error>> {
    let command = routing_command()?;
    let output = run_tool(&command, &["get", role])?;
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

/// Store the message, then ring the recipient's pane when it has one. The bell is a hint (R9),
/// so a ring failure only warns.
#[allow(clippy::too_many_arguments)]
fn deliver(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: i64,
    sender: &str,
    recipient: &str,
    kind: &str,
    body: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let (recipient, kind) = swarm::store::route(connection, session_id, sender, recipient, kind)?;
    let seq = swarm::store::send_message(connection, root, session_id, sender, &recipient, &kind, body)?;
    if let Some(pane) = swarm::store::pane_of(connection, session_id, &recipient)? {
        connection.execute("UPDATE message SET rung_at = unixepoch() WHERE seq = ?1", [seq])?;
        let ring = swarm::adapter::load(root, adapter_name)
            .and_then(|a| a.run("ring", &[("pane", &pane), ("text", "swarm: new message")]));
        if let Err(error) = ring {
            eprintln!("swarm: ring failed: {error}");
        }
    }
    Ok(seq)
}

fn add_agent(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: i64,
    agent_id: &str,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pane = if role == "orchestrator" {
        Some(swarm::adapter::load(root, adapter_name)?.run("self", &[])?)
    } else {
        None
    };
    swarm::store::add_agent(connection, session_id, agent_id, role)?;
    if let Some(pane) = pane
        && !pane.is_empty()
    {
        swarm::store::set_pane(connection, session_id, agent_id, &pane)?;
    }
    Ok(())
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
    swarm::store::add_agent(connection, session_id, agent_id, role)?;
    let adapter = swarm::adapter::load(root, &adapter_name())?;
    let session = session_id.to_string();
    let home = swarm::paths::home()?;
    let current_adapter = adapter_name();
    let vars = [("session_id", session.as_str()), ("agent_id", agent_id), ("home", home.as_str()), ("adapter", current_adapter.as_str())];
    let pane = adapter.run("spawn", &vars)?;
    swarm::store::set_pane(connection, session_id, agent_id, &pane)?;
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
    let pane = swarm::store::pane_of(&connection, session_id()?, agent_id)?.ok_or("swarm: no pane recorded")?;
    swarm::adapter::load(&root, &adapter_name())?.attach(&[("pane", &pane)])
}

/// A child ended without `swarm finish`: send a fallback summary in its name and queue a
/// summarize job, unless it already sent one (R19). Then forget its pane.
fn report_dead(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session_id: i64,
    child: &str,
    orchestrator: &str,
    note: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !swarm::store::has_summary(connection, session_id, child)? {
        deliver(connection, root, &adapter_name(), session_id, child, orchestrator, "summary", note)?;
        swarm::store::enqueue_job(connection, session_id, child, "summarize")?;
    }
    swarm::store::clear_pane(connection, session_id, child)
}

/// One sweep pass: re-ring unseen messages and report each child whose pane is gone.
fn sweep_once(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: i64,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for (child, pane) in swarm::store::live_children(connection, session_id, agent_id)? {
        if adapter.has_pane(&pane)? {
            if swarm::store::mark_unseen_for_rering(connection, session_id, &child, RERING_UNSEEN_AFTER_SECS)? {
                match adapter.run("ring", &[("pane", &pane), ("text", "swarm: new message")]) {
                    Ok(_) => eprintln!("swarm: re-ringed {child}"),
                    Err(error) => eprintln!("swarm: re-ring failed for {child}: {error}"),
                }
            }
            continue;
        }
        let note = format!("agent {child} died without a summary");
        report_dead(connection, root, session_id, &child, agent_id, &note)?;
        println!("dead {child}");
    }
    Ok(())
}

/// Feed the agent's captured log to the summarizer shell command and return its output.
/// The log is removed only after a successful run, so a retry still has its input.
fn summarize_log(log: &std::path::Path, summarizer: &str) -> Result<String, Box<dyn std::error::Error>> {
    let input = std::fs::File::open(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let output = std::process::Command::new("sh").arg("-c").arg(summarizer).stdin(input).output()?;
    if !output.status.success() {
        return Err(format!("summarizer failed: {}", String::from_utf8_lossy(&output.stderr).trim()).into());
    }
    std::fs::remove_file(log)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.first().map(String::as_str) == Some("init") {
        return init();
    }
    if let [cmd, json] = args && cmd == "roles" && json == "--json" {
        return print_json(&load_roles()?);
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
        println!("ok {name}");
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
        swarm::store::set_chair(&connection, session_id()?, parse_chair(value)?)?;
        return Ok(());
    }
    if let [cmd, sub, ids @ ..] = args
        && cmd == "session"
        && sub == "archive"
        && !ids.is_empty()
    {
        let ids = ids
            .iter()
            .map(|id| id.parse::<i64>().map_err(|_| USAGE))
            .collect::<Result<Vec<_>, _>>()?;
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
                    swarm::store::set_chair_log(&connection, row.id, path)?;
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
                agents: row.agents,
                messages: row.messages,
                last_message_at: row.last_message_at,
            });
        }
        return print_json(&swarm::bus::SessionList { sessions });
    }
    if let [cmd, sub, agent_id, role] = args && cmd == "agent" && sub == "add" {
        return add_agent(&connection, &root, &adapter_name(), session_id()?, agent_id, role);
    }
    if let [cmd, json] = args && cmd == "agents" && json == "--json" {
        let session_id = session_id()?;
        let rows = swarm::store::agents(&connection, session_id)?;
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
                swarm::bus::Agent { id: row.id, role: row.role, pane: row.pane, alive }
            })
            .collect();
        return print_json(&swarm::bus::AgentList { agents });
    }
    if let [cmd, rest @ ..] = args && cmd == "messages" {
        let after = match rest {
            [json] if json == "--json" => 0,
            [json, flag, seq] if json == "--json" && flag == "--after" => {
                seq.parse().map_err(|_| format!("swarm: bad seq {seq}"))?
            }
            _ => return Err(USAGE.into()),
        };
        let messages = swarm::store::messages(&connection, session_id()?, after)?
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
        let account = match rest {
            [] => None,
            [flag, account] if flag == "--account" => Some(account.as_str()),
            _ => return Err(USAGE.into()),
        };
        let resolved = resolve_role(role)?;
        let provider = resolved.provider.clone();
        let command = swarm::bus::argv(role, &resolved, &swarm::paths::home()?)?;
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
            let (session, agent, kind, attempts) = swarm::store::job(&connection, job_id)?;
            if kind != "summarize" {
                swarm::store::park_job(&connection, job_id)?;
                println!("parked {job_id} unknown kind {kind}");
                continue;
            }
            let log = root.join(format!("runs/{session}/{agent}.log"));
            match summarize_log(&log, &summarizer) {
                Ok(summary) => {
                    let orchestrator = swarm::store::orchestrator_of(&connection, session)?;
                    deliver(&mut connection, &root, &adapter_name(), session, &agent, &orchestrator, "summary", &summary)?;
                    swarm::store::finish_job(&connection, job_id)?;
                    println!("done {job_id}");
                }
                Err(error) if attempts < 3 => {
                    swarm::store::release_job(&connection, job_id, 30)?;
                    println!("retry {job_id}: {error}");
                }
                Err(error) => {
                    swarm::store::park_job(&connection, job_id)?;
                    println!("parked {job_id}: {error}");
                }
            }
        }
        return Ok(());
    }
    if let [cmd, child] = args && cmd == "close" {
        let session_id = session_id()?;
        let pane = swarm::store::pane_of(&connection, session_id, child)?.ok_or("swarm: no pane recorded")?;
        swarm::adapter::load(&root, &adapter_name())?.run("close", &[("pane", &pane)])?;
        return swarm::store::clear_pane(&connection, session_id, child);
    }
    if let [cmd, agent_id] = args && cmd == "type" {
        let text = std::io::read_to_string(std::io::stdin())?;
        if text.trim().is_empty() {
            return Err("swarm: empty text".into());
        }
        let pane = swarm::store::pane_of(&connection, session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
        swarm::adapter::load(&root, &adapter_name())?
            .run("ring", &[("pane", &pane), ("text", &text)])?;
        return Ok(());
    }
    if let [cmd, agent_id] = args && cmd == "interrupt" {
        let pane = swarm::store::pane_of(&connection, session_id()?, agent_id)?
            .ok_or("swarm: no pane recorded")?;
        swarm::adapter::load(&root, &adapter_name())?.run("interrupt", &[("pane", &pane)])?;
        return Ok(());
    }
    let (session_id, agent_id) = identity()?;
    match args {
        [cmd, recipient, kind] if cmd == "send" => {
            let body = std::io::read_to_string(std::io::stdin())?;
            let seq = deliver(&mut connection, &root, &adapter_name(), session_id, &agent_id, recipient, kind, &body)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "finish" => {
            let summary = std::io::read_to_string(std::io::stdin())?;
            let orchestrator = swarm::store::orchestrator_of(&connection, session_id)?;
            let seq = deliver(&mut connection, &root, &adapter_name(), session_id, &agent_id, &orchestrator, "summary", &summary)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "exited" => {
            let pane = swarm::store::pane_of(&connection, session_id, &agent_id)?.ok_or("swarm: no pane recorded")?;
            let text = swarm::adapter::load(&root, &adapter_name())?.run("capture", &[("pane", &pane)])?;
            let run_dir = root.join(format!("runs/{session_id}"));
            std::fs::create_dir_all(&run_dir)?;
            swarm::store::write_atomic(&run_dir.join(format!("{agent_id}.log")), &text)?;
            let orchestrator = swarm::store::orchestrator_of(&connection, session_id)?;
            let note = format!("agent {agent_id} exited without a summary");
            report_dead(&mut connection, &root, session_id, &agent_id, &orchestrator, &note)
        }
        [cmd, rest @ ..] if cmd == "sweep" => {
            let every = match rest {
                [] => None,
                [flag, secs] if flag == "--every" => Some(secs.parse::<u64>().ok().filter(|s| *s > 0).ok_or(USAGE)?),
                _ => return Err(USAGE.into()),
            };
            let adapter = swarm::adapter::load(&root, &adapter_name())?;
            loop {
                match sweep_once(&mut connection, &root, &adapter, session_id, &agent_id) {
                    Ok(()) => {}
                    Err(error) if every.is_some() => eprintln!("swarm: sweep skipped: {error}"),
                    Err(error) => return Err(error),
                }
                let Some(secs) = every else { return Ok(()) };
                std::thread::sleep(std::time::Duration::from_secs(secs));
            }
        }
        [cmd] if cmd == "inbox" => {
            for m in swarm::store::inbox(&connection, session_id, &agent_id)? {
                println!("{} {} {} {}", m.seq, m.sender_id, m.kind, m.body_path);
            }
            Ok(())
        }
        [cmd, seq] if cmd == "ack" => {
            let seq: i64 = seq.parse().map_err(|_| format!("swarm: bad seq {seq}"))?;
            swarm::store::ack(&connection, session_id, seq, &agent_id)
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
    fn sweep_rerings_a_child_once_for_old_unseen_messages() {
        let root = std::env::temp_dir().join(format!("swarm-sweep-test-{}", std::process::id()));
        let ring_log = root.join("rings");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, session, CODER, "%2").unwrap();
        swarm::store::send_message(&mut connection, &root, session, ORCHESTRATOR, CODER, "ask", "one").unwrap();
        swarm::store::send_message(&mut connection, &root, session, ORCHESTRATOR, CODER, "ask", "two").unwrap();
        connection.execute("UPDATE message SET created_at = unixepoch() - 16", []).unwrap();

        let adapter = swarm::adapter::parse(
            "fake",
            &format!("self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = echo %2\nclose = true\ncapture = true\n", ring_log.display()),
        )
        .unwrap();
        sweep_once(&mut connection, &root, &adapter, session, ORCHESTRATOR).unwrap();
        sweep_once(&mut connection, &root, &adapter, session, ORCHESTRATOR).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "%2:swarm: new message\n");
        connection.execute("UPDATE message SET rung_at = unixepoch() - 16", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, session, ORCHESTRATOR).unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), "%2:swarm: new message\n%2:swarm: new message\n");

        swarm::store::inbox(&connection, session, CODER).unwrap();
        connection.execute("UPDATE message SET rung_at = unixepoch() - 16", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, session, ORCHESTRATOR).unwrap();
        swarm::store::ack(&connection, session, 1, CODER).unwrap();
        connection.execute("UPDATE message SET seen_at = NULL, rung_at = unixepoch() - 16 WHERE seq = 1", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, session, ORCHESTRATOR).unwrap();

        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), "%2:swarm: new message\n%2:swarm: new message\n");
    }

    #[test]
    fn sweep_reports_a_dead_child_as_before() {
        let root = std::env::temp_dir().join(format!("swarm-dead-test-{}", std::process::id()));
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane", std::path::Path::new("/test"), None, None).unwrap();
        swarm::store::add_agent(&connection, session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, session, CODER, "coder").unwrap();
        swarm::store::set_pane(&connection, session, CODER, "%2").unwrap();
        let adapter = swarm::adapter::parse(
            "fake",
            "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
        )
        .unwrap();

        sweep_once(&mut connection, &root, &adapter, session, ORCHESTRATOR).unwrap();

        assert_eq!(swarm::store::pane_of(&connection, session, CODER).unwrap(), None);
        assert!(swarm::store::has_summary(&connection, session, CODER).unwrap());
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

        add_agent(&connection, &root, "fake", session, ORCHESTRATOR, "orchestrator").unwrap();
        swarm::store::add_agent(&connection, session, CODER, "coder").unwrap();
        deliver(&mut connection, &root, "fake", session, CODER, ORCHESTRATOR, "summary", "done").unwrap();

        assert_eq!(swarm::store::pane_of(&connection, session, ORCHESTRATOR).unwrap().as_deref(), Some("%9"));
        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), "%9:swarm: new message\n");
    }
}
