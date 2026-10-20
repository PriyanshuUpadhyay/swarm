//! CLI commands session.

use super::*;
pub(crate) fn commands_session(
    args: &[String],
    root: &std::path::Path,
    connection: &mut rusqlite::Connection,
) -> Result<Option<()>, Box<dyn std::error::Error>> {
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
            connection,
            talk_mode,
            &env::current_dir()?,
            chair
                .as_ref()
                .map(|(provider, id)| (provider.as_str(), id.as_str())),
            Some(&adapter),
        )?;
        record_pi_log(
            connection,
            &session,
            chair.as_ref().map(|(p, id)| (p.as_str(), id.as_str())),
        )?;
        // A chair in a Herdr, Superset, Rex, or tmux pane is registered here, so it needs no ids of
        // its own. The app's `tmux-swarm` chair is launched as an agent instead. A child pane
        // already has an agent id, and registering it would take its pane from its own session.
        if env::var_os("SWARM_AGENT_ID").is_none()
            && matches!(adapter.as_str(), "herdr" | "superset" | "rex" | "tmux")
            && own_pane().is_ok()
        {
            add_agent(
                connection,
                root,
                &adapter,
                &session,
                "orchestrator",
                "orchestrator",
            )?;
        }
        println!("{session}");
        return Ok(Some(()));
    }
    if let [cmd, sub, value] = args
        && cmd == "session"
        && sub == "chair"
    {
        let (session_id, agent_id) = identity()?;
        let chair = parse_chair(value)?;
        set_chair_for_caller(connection, &session_id, &agent_id, chair)?;
        record_pi_log(connection, &session_id, chair)?;
        return Ok(Some(()));
    }
    if let [cmd, sub, new_id, old_id] = args
        && cmd == "session"
        && sub == "continue"
    {
        return swarm::store::continue_session(connection, new_id, old_id).map(Some);
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
            swarm::store::archive_sessions(connection, &ids)?;
        } else {
            swarm::store::unarchive_sessions(connection, &ids)?;
        }
        return Ok(Some(()));
    }
    if let [cmd, json, flags @ ..] = args
        && cmd == "sessions"
        && json == "--json"
        && (flags.is_empty() || matches!(flags, [flag] if flag == "--archived"))
    {
        let mut sessions = Vec::new();
        for row in swarm::store::sessions_with_archived(connection, !flags.is_empty())? {
            let chair_log = resolved_chair_log(&row);
            if let Some(path) = &chair_log {
                let path_text = path.to_string_lossy();
                if row.chair_log.as_deref() != Some(path_text.as_ref()) {
                    swarm::store::set_chair_log(connection, &row.id, path)?;
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
        return print_json(&swarm::bus::SessionList { sessions }).map(Some);
    }
    if let [cmd, sub, agent_id, role] = args
        && cmd == "agent"
        && sub == "add"
    {
        return add_agent(
            connection,
            root,
            &adapter_name(),
            &session_id()?,
            agent_id,
            role,
        )
        .map(Some);
    }
    if let [cmd, json] = args
        && cmd == "agents"
        && json == "--json"
    {
        let session_id = session_id()?;
        let adapter = swarm::adapter::load(root, &adapter_name())?;
        return print_json(&list_agents(connection, root, &session_id, &adapter)?).map(Some);
    }
    if let [cmd, json, all] = args
        && cmd == "agents"
        && json == "--json"
        && all == "--all"
    {
        // SwarmCLIBus gives CapturedProcess 20 s; reserve 2 s for startup, DB work, and JSON.
        let listings = all_agent_listings(connection, root, std::time::Duration::from_secs(18))?;
        return print_json(&listings).map(Some);
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
        let messages = swarm::store::messages(connection, &session_id()?, after)?
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
        return print_json(&swarm::bus::MessageList { messages }).map(Some);
    }
    Ok(None)
}
