// Why a caller may not launch, revert, set up, sweep, or use a model.

/// Why a caller may not launch an agent, or None. A pane `swarm spawn` made carries its own
/// agent id, and only the orchestrator starts children; Herdr marks its own agent panes too.
pub fn launch_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane)
        .then_some("swarm: a child agent cannot launch agents; ask the orchestrator")
}

/// Why a caller may not revert the owner's managed changes, or None: the same child panes that
/// may not launch, because a revert removes the guard registration that blocks their own calls.
pub fn revert_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane).then_some(
        "swarm: a child agent cannot revert the owner's managed changes; ask the orchestrator",
    )
}

/// Why a caller may not apply `swarm setup`, or None: the same child panes, because an apply
/// writes the owner's launch consent, so a seat left on `ask` could give itself standing consent.
pub fn setup_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane)
        .then_some("swarm: a child agent cannot apply swarm setup; ask the orchestrator")
}

/// Why `swarm sweep --every` may not run on `adapter`, or None. The app's listing already rings an
/// idle chair again, and a chair whose CLI has no background task (pi) waits on the loop, so its
/// CLI queues every ring until someone stops the command.
pub fn sweep_refusal(adapter: &str, every: bool) -> Option<&'static str> {
    (every && adapter == "tmux-swarm").then_some(
        "swarm: the app rings an idle chair again, and a chair that waits on `swarm sweep --every` \
         gets no ring until the loop ends; end the turn instead",
    )
}

fn child_agent(swarm_agent: Option<&str>, herdr_agent_pane: Option<&str>) -> bool {
    swarm_agent.is_some_and(|agent| agent != "orchestrator") || herdr_agent_pane == Some("1")
}

/// Fable runs as a child only for review and council seats. The router refuses it too; this is
/// the second, independent check, so a config edited past the router still cannot reach a pane.
pub fn fable_refusal(agent_id: &str, role: &str, model: Option<&str>) -> Option<String> {
    let fable = model.is_some_and(|model| model.to_ascii_lowercase().contains("fable"));
    (fable
        && agent_id != "orchestrator"
        && !role.starts_with("review.")
        && !role.starts_with("council."))
    .then(|| format!("swarm: Fable is a child only for review.* and council.* (role {role})"))
}

/// A model absent from a readable list, with the account and model choices in list order.
pub fn model_refusal(
    provider: &str,
    model: &str,
    account: Option<&str>,
    other_accounts: &[String],
    models: &[String],
) -> String {
    let login = account.map_or_else(
        || "the CLI's own login".to_string(),
        |name| format!("account {name}"),
    );
    let mut message = format!(
        "swarm: model {model} is not in the {provider} model list for {login}\nfixes that might work:"
    );
    if !other_accounts.is_empty() {
        let accounts = other_accounts
            .iter()
            .map(|name| format!("--account {name}"))
            .collect::<Vec<_>>()
            .join(", ");
        message.push_str(&format!("\n  - another account: {accounts}"));
    }
    message.push_str(&format!("\n  - another model: {}", models.join(", ")));
    message
}

/// Why this session has no spawn path, from the stderr of `herdr status`, or None. A sandbox that
/// denies the Herdr socket denies every later `herdr` call too, and a wider sandbox would spend a
/// user approval on a path this session is not meant to use.
pub fn socket_refusal(herdr_status_stderr: &str) -> Option<&'static str> {
    herdr_status_stderr.contains("PermissionDenied").then_some(
        "swarm: this sandbox denies the Herdr socket, so this session has no spawn path; report that and stop, and do not request a wider sandbox",
    )
}
