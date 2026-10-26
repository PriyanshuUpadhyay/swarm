//! CLI commands launch.

use super::*;
use rusqlite::OptionalExtension;
pub(crate) fn commands_launch(
    args: &[String],
    root: &std::path::Path,
    connection: &mut rusqlite::Connection,
) -> Result<Option<()>, Box<dyn std::error::Error>> {
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
        // `--resume` starts the chat's own CLI session again, with its whole context (ADR 0107).
        let resume = options.iter().any(|arg| arg == "--resume");
        let options: Vec<String> = options
            .iter()
            .filter(|arg| *arg != "--resume")
            .cloned()
            .collect();
        let (mut account, mut cwd, mut requested_provider, mut requested_model, mut effort) =
            (Some("auto"), None, None, None, None);
        for pair in options.chunks(2) {
            match pair {
                [flag, value] if flag == "--account" => account = Some(value.as_str()),
                [flag, value] if flag == "--cwd" => cwd = Some(std::path::PathBuf::from(value)),
                [flag, value] if flag == "--provider" && Provider::parse(value).is_some() => {
                    requested_provider = Some(value.as_str())
                }
                [flag, value] if flag == "--model" => requested_model = Some(value.as_str()),
                [flag, value] if flag == "--effort" => effort = Some(value.as_str()),
                _ => return Err(USAGE.into()),
            }
        }
        let resume = match resume {
            false => None,
            true if agent_id != "orchestrator"
                || requested_provider.is_some()
                || !extra.is_empty() =>
            {
                return Err(
                    "swarm: --resume takes the chair and its own provider, with no extra args"
                        .into(),
                );
            }
            true => {
                let session = session_id()?;
                // A pane that ended with its host, such as a stopped tmux server, ran no exit
                // hook, so its id stays on the chair. The adapter's listing is the truth.
                let pane = swarm::store::agents(connection, &session)?
                    .into_iter()
                    .find(|agent| agent.id == "orchestrator")
                    .and_then(|agent| agent.pane);
                if let Some(pane) = pane
                    && !swarm::adapter::load(root, &adapter_name())?.has_pane(&pane)?
                {
                    swarm::store::clear_pane(connection, &session, "orchestrator")?;
                }
                Some(resume_target(connection, &session)?)
            }
        };
        // A plain Resume runs the role's runner (ADR 0107). Only a chat whose profile has no
        // runner for its CLI, such as pi or AGY, runs its last model and level (ADR 0110). The
        // stored model is the transcript id, which drops a `[1m]` suffix.
        let no_runner = |provider: Provider| {
            swarm::config::load().is_ok_and(|(config, _)| {
                config
                    .profile(role)
                    .is_ok_and(|profile| !profile.runners.iter().any(|r| r.provider == provider))
            })
        };
        let stored: Option<(Option<String>, Option<String>)> = match (&resume, requested_model) {
            (Some((provider, _)), None) if role == "chat" && no_runner(*provider) => connection
                .query_row(
                    "SELECT model, effort FROM agent WHERE session_id = ?1 AND id = 'orchestrator'",
                    [session_id()?],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?,
            _ => None,
        };
        if let Some((model, stored_effort)) = &stored
            && let Some(model) = model
        {
            requested_model = Some(model.as_str());
            effort = effort.or(stored_effort.as_deref());
        }
        if let Some((provider, _)) = &resume {
            requested_provider = Some(provider.id());
        }
        let cwd = cwd.map_or_else(env::current_dir, Ok)?;
        let cwd = std::fs::canonicalize(&cwd)
            .map_err(|error| format!("swarm: bad --cwd {}: {error}", cwd.display()))?;
        let (resolved, probe): (swarm::bus::ResolvedRole, Probe) = match requested_model {
            _ if role != "chat" && (requested_model.is_some() || effort.is_some()) => {
                return Err("swarm: --model and --effort require the chat role".into());
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
        // A switch keeps the chat and changes only its run, so the effort is the owner's pick.
        let mut resolved = resolved;
        // pi's `default` hides which login it runs on, so the chat shows no account to switch.
        if resolved.provider.as_deref() == Some("pi")
            && resolved.model.as_deref() == Some("default")
            && let Some(model) = swarm::providers::pi_default_model()
        {
            resolved.model = Some(model);
        }
        if let Some(effort) = effort {
            let kind = resolved.provider.as_deref().and_then(Provider::parse);
            if !kind.is_some_and(|kind| kind.info().efforts.contains(&effort)) {
                return Err(format!("swarm: --effort {effort} is not a level of this CLI").into());
            }
            resolved.effort = Some(effort.into());
        }
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
        if let Some((_, id)) = &resume {
            extra = resume_args(kind, id);
        }
        // Trust and pane environment must use the same native pick.
        // A provider with no account source launches on its CLI's own login, so `auto` means
        // nothing there; the chat profile passes it whichever runner starts.
        let picked = match (account, kind.account_provider(&runner.model)) {
            (Some(requested), Some(account_kind)) => {
                let mut cache = probe.accounts.borrow_mut();
                let accounts = cache.entry(account_kind).or_insert_with(|| {
                    pick_accounts(account_kind, account_deadline, requested == "auto")
                });
                // Codex keeps a session in one account's home. Auto stays there; a named other
                // account gets a copy of the session first, so the chat moves with its history.
                let (owner, moved) = match &resume {
                    Some((Provider::Codex, id)) => {
                        let list = accounts
                            .as_ref()
                            .map_err(|error| format!("swarm: {error}"))?;
                        let (owner, rollout) =
                            swarm::profiles::native::codex_rollout(&list.accounts, id).ok_or_else(
                                || format!("swarm: session {id} is not in any codex account"),
                            )?;
                        if requested == "auto" || requested == owner.name {
                            (Some(owner.name.clone()), None)
                        } else {
                            (None, Some((owner.home.clone(), rollout)))
                        }
                    }
                    _ => (None, None),
                };
                let picked = select_account(
                    account_kind,
                    owner.as_deref().unwrap_or(requested),
                    resolved.model.as_deref(),
                    probe.min_usage_left_pct,
                    accounts.as_ref().map_err(Clone::clone),
                )?;
                if let (Some((from, rollout)), Some(to)) = (&moved, &picked) {
                    move_codex_session(connection, from, rollout, &to.home)?;
                }
                // pi reads a Codex account from `PI_CODEX_HOME`, not `CODEX_HOME` (ADR 0081).
                if kind == Provider::Pi && account_kind == Provider::Codex {
                    picked.map(|mut account| {
                        account.env = swarm::providers::pi_codex_env(&account.name, &account.home);
                        account
                    })
                } else {
                    picked
                }
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
            store: connection,
            consent,
            // Auto keeps the base rule for a launch with no account: a file that cannot take the
            // entry is a warning. Only a named account must get it.
            required: picked.is_some() && account.is_some_and(|name| name != "auto"),
            cwd: &cwd,
        };
        // Codex and AGY have no per-launch hook to name their chair, and the app lists and reads a
        // chat only by its chair provider. Their session id comes later with their log.
        if matches!(kind, Provider::Codex | Provider::Agy) && agent_id == "orchestrator" {
            // A new chair has a new session, so the old id goes; a hook keeps a log only when it
            // names the chair's id (set_chair_log).
            if resume.is_none() {
                swarm::store::set_chair(connection, &session_id()?, None)?;
            }
            swarm::store::set_chair_provider(connection, &session_id()?, kind.id())?;
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
                // A resume opens the stored session, which `session new` may have named.
                let id = match &resume {
                    Some((_, id)) => id.clone(),
                    None => format!("{session}-{agent_id}"),
                };
                let mut args = vec!["--session-id".to_string(), id.clone()];
                if let Some(context) = pane_context(kind.id(), &session, agent_id, role) {
                    args.extend(["--append-system-prompt".into(), context]);
                }
                extra.splice(0..0, args);
                if agent_id == "orchestrator" {
                    swarm::store::set_chair(connection, &session, Some((kind.id(), &id)))?;
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
        spawn_agent(
            connection,
            root,
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
        )?;
        // A resumed chat is open again, and an imported one now has panes the app reaches.
        if resume.is_some() {
            let session = session_id()?;
            swarm::store::unarchive_sessions(connection, std::slice::from_ref(&session))?;
            connection.execute(
                "UPDATE session SET adapter = ?2 WHERE id = ?1 AND adapter IS NULL",
                (&session, adapter_name()),
            )?;
        }
        return Ok(Some(()));
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
            connection,
            root,
            agent_id,
            role,
            options,
            account_pick_deadline(),
        )
        .map(Some);
    }
    Ok(None)
}
