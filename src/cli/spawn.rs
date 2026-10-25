//! CLI spawn.

use super::*;

pub(crate) struct SpawnOptions<'a> {
    pub(crate) provider: Option<&'a str>,
    pub(crate) account: Option<&'a str>,
    pub(crate) resolved_account: Option<&'a swarm::profiles::Account>,
    pub(crate) command: &'a [String],
    pub(crate) runner: Option<&'a swarm::config::Runner>,
}

pub(crate) fn parse_spawn_options(args: &[String]) -> Result<SpawnOptions<'_>, String> {
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

pub(crate) fn add_agent(
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
pub(crate) fn register_spawned_pane(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent_id: &str,
    role: &str,
    provider: Option<&str>,
    vars: &[(&str, &str)],
    launch: Option<(&swarm::config::Runner, Option<&str>)>,
    line: impl FnOnce() -> Result<Option<String>, Box<dyn std::error::Error>>,
) -> Result<(String, Option<String>), Box<dyn std::error::Error>> {
    // A chair whose pane ended takes its row back, as `launch --resume` does (ADR 0107). Any other
    // id that the session holds is still refused by the insert.
    let ended = agent_id == "orchestrator"
        && swarm::store::agents(connection, session_id)?
            .iter()
            .any(|agent| agent.id == agent_id && agent.pane.is_none());
    if ended {
        connection.execute(
            "UPDATE agent SET role = ?3 WHERE session_id = ?1 AND id = ?2",
            (session_id, agent_id, role),
        )?;
    } else {
        swarm::store::add_agent(connection, session_id, agent_id, role)?;
    }
    if let Some(provider) = provider {
        swarm::store::set_provider(connection, session_id, agent_id, provider)?;
    }
    // The launch line is written only after the insert above refused no live id, so a duplicate
    // spawn leaves the live seat's script alone. A host with `run` makes it the pane's program.
    let text = match line() {
        Ok(text) => text,
        Err(error) => {
            if !ended {
                swarm::store::remove_agent(connection, session_id, agent_id)?;
            }
            return Err(error);
        }
    };
    let runs = text.is_some() && adapter.run_line.is_some();
    let mut vars = vars.to_vec();
    if let Some(text) = text.as_deref().filter(|_| runs) {
        vars.push(("command", text));
    }
    let verb = if runs { "run" } else { "spawn" };
    let pane = match adapter.run(verb, &vars) {
        Ok(pane) => pane,
        Err(error) => {
            if !ended {
                swarm::store::remove_agent(connection, session_id, agent_id)?;
            }
            return Err(error);
        }
    };
    swarm::store::set_pane(connection, session_id, agent_id, &pane)?;
    if let Some((runner, account)) = launch {
        swarm::store::set_launch(connection, session_id, agent_id, role, runner, account)?;
    }
    let ring = text.filter(|_| !runs);
    Ok((pane, ring))
}

pub(crate) fn list_models(
    provider: &str,
) -> Result<swarm::providers::ModelList, Box<dyn std::error::Error>> {
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
pub(crate) fn trust_target(
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

pub(crate) fn spawn_agent(
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
    // A host that stacks seats anchors on these, so a seat never lands beside another
    // session's seats on the same server.
    let panes = swarm::store::live_children(connection, &session_id, "orchestrator")?
        .into_iter()
        .map(|(_, pane)| pane)
        .collect::<Vec<_>>()
        .join(" ");
    let vars = [
        ("session_id", session.as_str()),
        ("agent_id", agent_id),
        ("home", home.as_str()),
        ("adapter", current_adapter.as_str()),
        ("panes", panes.as_str()),
    ];
    let line = || {
        if options.command.is_empty() {
            return Ok(None);
        }
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
        let hook = swarm::adapter::shell_line(&[exe.clone(), "exited".into()]);
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
        // tmux answers no OSC 7501 probe, so a CLI that sends reports gets a terminal that does
        // when the pane feeds `swarm status-feed`; Codex sends none (ADR 0122).
        let child = if adapter.spawn.contains("SWARM_FEED")
            && matches!(
                provider,
                Some(Provider::Claude | Provider::Agy | Provider::Pi)
            ) {
            let pty = swarm::adapter::shell_line(&[exe, "pty".into(), "--".into()]);
            format!("{pty} {child}")
        } else {
            child
        };
        let line = script_line(root, &session_id, agent_id, &format!("{path}{child}"))?;
        Ok(Some(format!("{line}; {hook}")))
    };
    let (pane, ring) = register_spawned_pane(
        connection,
        &adapter,
        &session_id,
        agent_id,
        role,
        provider.map(Provider::id),
        &vars,
        options.runner.map(|runner| (runner, options.account)),
        line,
    )?;
    if let Some(text) = ring {
        adapter.run("ring", &[("pane", &pane), ("text", &text)])?;
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
pub(crate) fn swarm_bin(
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
pub(crate) fn script_line(
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

#[cfg(test)]
#[path = "spawn/tests.rs"]
mod tests;
