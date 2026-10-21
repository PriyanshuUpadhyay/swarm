//! CLI commands pane.

use super::*;
pub(crate) fn commands_pane(
    args: &[String],
    root: &std::path::Path,
    connection: &mut rusqlite::Connection,
) -> Result<Option<()>, Box<dyn std::error::Error>> {
    if let [cmd, child] = args
        && cmd == "close"
    {
        let session_id = session_id()?;
        let pane = swarm::store::require_pane(connection, &session_id, child)?;
        swarm::adapter::load(root, &adapter_name())?.run("close", &[("pane", &pane)])?;
        return swarm::store::clear_pane(connection, &session_id, child).map(Some);
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
        let agent = swarm::store::agents(connection, &session_id)?
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
        ))
        .map(Some);
    }
    if let [cmd, agent_id] = args
        && cmd == "type"
    {
        let text = std::io::read_to_string(std::io::stdin())?;
        if text.trim().is_empty() {
            return Err("swarm: empty text".into());
        }
        let session_id = session_id()?;
        let pane = swarm::store::require_pane(connection, &session_id, agent_id)?;
        let adapter = swarm::adapter::load(root, &adapter_name())?;
        if !adapter.has_pane(&pane)? {
            return Err(
                "swarm: this chat's pane has closed; start a new chat or switch model".into(),
            );
        }
        // Marked before typing, which takes a second or more, so the row turns at once.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;
        swarm::store::mark_sent(connection, &session_id, agent_id, Some(now))?;
        let typed = type_text(connection, &adapter, &session_id, agent_id, &pane, &text);
        if typed.is_err() {
            swarm::store::mark_sent(connection, &session_id, agent_id, None)?;
        }
        return typed.map(Some);
    }
    Ok(None)
}
