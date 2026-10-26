//! CLI answer.

use super::*;
pub(crate) fn answer(
    args: &[String],
    root: &std::path::Path,
    connection: &mut rusqlite::Connection,
) -> Result<Option<()>, Box<dyn std::error::Error>> {
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
        let session = session_id()?;
        let pane = swarm::store::require_pane(connection, &session, agent_id)?;
        let agent = swarm::store::agents(connection, &session)?
            .into_iter()
            .find(|agent| agent.id == *agent_id)
            .ok_or("swarm: unknown agent")?;
        let provider = swarm::store::provider_of(connection, &session, agent_id)?;
        let adapter = swarm::adapter::load(root, &adapter_name())?;
        // One answer per pane at a time, so the chair's card and the column's card cannot mix
        // their keys. The lock ends with this process.
        let lock =
            std::fs::File::create(root.join(format!("answer-{}.lock", pane.replace('/', "_"))))?;
        lock.try_lock()
            .map_err(|_| format!("swarm: an answer to {agent_id} is still being sent"))?;
        let read = || {
            let pane = [("pane", pane.as_str())];
            let screen = adapter.screen(&pane, std::time::Duration::from_secs(1))?;
            swarm::screen::agent_prompt(
                provider.as_deref(),
                &screen,
                || adapter.capture_within(&pane, std::time::Duration::from_secs(1)),
                agent.log.as_deref().map(std::path::Path::new),
            )
        };
        let prompt = read().ok_or(format!("swarm: {agent_id} shows no question now"))?;
        let request = if provider.as_deref() == Some("claude") {
            swarm::permission::permission_for_prompt(
                root,
                &session,
                agent_id,
                &prompt,
                permission_now_ms()?,
            )?
        } else {
            None
        };
        let current_id = match &request {
            Some(request) => format!("{}.{}", prompt.id, request.id),
            None => prompt.id.clone(),
        };
        if current_id != *prompt_id {
            return Err(if prompt_id.contains('.') {
                "swarm: this permission request expired or was already answered; read it again"
            } else {
                "swarm: the question changed; read it again"
            }
            .into());
        }
        let keys = prompt
            .keys(choice)
            .ok_or(format!("swarm: the question has no choice {choice}"))?;
        if let Some(request) = request {
            let decision = swarm::permission::decision(&request.payload, &prompt.choices[choice]);
            swarm::permission::answer_permission_request(
                root,
                &session,
                agent_id,
                &request.id,
                decision.as_ref(),
                permission_now_ms()?,
            )?;
            if decision.is_some() {
                return Ok(Some(()));
            }
        }
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
        return Ok(Some(()));
    }
    Ok(None)
}
