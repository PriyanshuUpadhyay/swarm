//! The Claude permission handoff on the CLI side: the hook's wait and the listing's prompt id.

pub(crate) fn wait_for_permission(
    root: &std::path::Path,
    session: &str,
    agent: &str,
    payload: &serde_json::Value,
) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
    let wait = std::time::Duration::from_secs(swarm::permission::WAIT_SECONDS);
    let deadline = std::time::Instant::now() + wait;
    let now = permission_now_ms()?;
    let id = swarm::permission::add_permission_request(
        root,
        session,
        agent,
        payload,
        now + wait.as_millis() as i64,
    )?;
    let result = (|| {
        while std::time::Instant::now() < deadline {
            match swarm::permission::poll_permission_request(
                root,
                session,
                &id,
                permission_now_ms()?,
            )? {
                swarm::permission::PermissionPoll::Waiting => {}
                swarm::permission::PermissionPoll::Decision(decision) => {
                    return Ok(Some(swarm::permission::output(decision)));
                }
                swarm::permission::PermissionPoll::Closed => return Ok(None),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        swarm::permission::remove_permission_request(root, session, &id)?;
        Ok(None)
    })();
    if result.is_err() {
        let _ = swarm::permission::remove_permission_request(root, session, &id);
    }
    result
}

pub(crate) fn permission_now_ms() -> Result<i64, Box<dyn std::error::Error>> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

/// The app's prompt id carries the waiting request's id, so a stale card cannot answer a new
/// request. A broken or ambiguous handoff keeps the plain prompt; `swarm answer` reports it.
pub(crate) fn tag_permission_prompt(
    root: &std::path::Path,
    session: &str,
    agent: &str,
    prompt: &mut swarm::screen::Prompt,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(Some(request)) =
        swarm::permission::permission_for_prompt(root, session, agent, prompt, permission_now_ms()?)
    {
        prompt.id = format!("{}.{}", prompt.id, request.id);
    }
    Ok(())
}
