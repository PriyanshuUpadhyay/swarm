//! CLI commands info.

use super::*;
pub(crate) fn commands_info(args: &[String]) -> Result<Option<()>, Box<dyn std::error::Error>> {
    if args.first().is_some_and(|arg| arg == "launch")
        && let Some(home) = env::var_os("HOME")
        && let Some(notice) =
            swarm::plugin::stale_notice(&stale_plugin_copies(&std::path::PathBuf::from(home)))
    {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{notice}");
    }
    if let [cmd, rest @ ..] = args
        && cmd == "run"
    {
        return run_cli(rest).map(Some);
    }

    #[cfg(feature = "serve")]
    match args.first().map(String::as_str) {
        Some("serve") => return serve::run(&args[1..]).map(Some),
        Some("ui") => return serve::ui(&args[1..]).map(Some),
        Some("pages") => return serve::pages(&args[1..]).map(Some),
        _ => {}
    }
    if let [cmd, session_id, agent_id] = args
        && cmd == "focus"
    {
        return focus(session_id, agent_id).map(Some);
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
        return Ok(Some(()));
    }
    if args.first().map(String::as_str) == Some("init") {
        return init().map(Some);
    }
    if args.first().map(String::as_str) == Some("setup") {
        return setup(&args[1..]).map(Some);
    }
    if args.first().map(String::as_str) == Some("doctor") {
        return doctor(&args[1..]).map(Some);
    }
    if args.first().map(String::as_str) == Some("hooks") {
        return hooks(&args[1..]).map(Some);
    }
    if args.first().map(String::as_str) == Some("managed") {
        return managed(&args[1..]).map(Some);
    }
    if let [cmd, json] = args
        && cmd == "roles"
        && json == "--json"
    {
        return print_json(&swarm::config::listing().map_err(|error| format!("swarm: {error}"))?)
            .map(Some);
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
        return Ok(Some(()));
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
            })
            .map(Some);
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
        return Ok(Some(()));
    }
    if let [cmd, json] = args
        && cmd == "notices"
        && json == "--json"
    {
        let mut connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
        return print_json(&swarm::store::take_notices(&mut connection)?).map(Some);
    }
    if let [cmd, due] = args
        && cmd == "notices"
        && due == "--due"
    {
        return notices_due().map(Some);
    }
    if let [cmd, rest @ ..] = args
        && cmd == "notices"
    {
        return notices_command(rest).map(Some);
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
        return Ok(Some(()));
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
                swarm::host::context(provider, |name| match name {
                    "SWARM_ROLE" => env::var("SWARM_SESSION_ID")
                        .ok()
                        .zip(env::var("SWARM_AGENT_ID").ok())
                        .and_then(|(session, agent)| seat_role(&session, &agent)),
                    _ => env::var(name).ok(),
                })
                .as_deref()
            )
        );
        return Ok(Some(()));
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
        return Ok(Some(()));
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
        return print_json(&serde_json::json!({ "profiles": profiles })).map(Some);
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
        return print_json(&serde_json::json!({ "revision": revision })).map(Some);
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
        return print_json(&serde_json::json!({ "revision": revision })).map(Some);
    }
    if let [cmd, json] = args
        && cmd == "providers"
        && json == "--json"
    {
        return print_json(&swarm::providers::info_list()).map(Some);
    }
    if let [cmd, provider_flag, provider, json] = args
        && cmd == "models"
        && provider_flag == "--provider"
        && json == "--json"
    {
        return print_json(&list_models(provider)?).map(Some);
    }
    if let [cmd, sub, rest @ ..] = args
        && cmd == "accounts"
        && sub == "login"
    {
        return account_login(rest).map(Some);
    }
    if let [cmd, sub, rest @ ..] = args
        && cmd == "accounts"
        && sub == "pick"
    {
        return account_pick(rest).map(Some);
    }
    if let [cmd, sub, revision_flag, revision, json] = args
        && cmd == "accounts"
        && sub == "reset"
        && revision_flag == "--revision"
        && json == "--json"
    {
        return print_json(&serde_json::json!({"revision":swarm::accounts::reset(revision)?}))
            .map(Some);
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
        })
        .map(Some);
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
        }))
        .map(Some);
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
        })
        .map(Some);
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
        ))
        .map(Some);
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
        })
        .map(Some);
    }
    Ok(None)
}
