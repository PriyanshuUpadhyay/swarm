// Host detection, session contracts, and provider hook output.

use std::io::Write;

use super::{
    APP_CONTEXT, APP_WORKER_CONTEXT, HERDR_CONTEXT, HERDR_WORKER_CONTEXT, REX_CONTEXT,
    REX_WORKER_CONTEXT, SUPERSET_CONTEXT, SUPERSET_WORKER_CONTEXT,
};

/// Whether this process runs in a Herdr pane.
fn in_herdr(env: impl Fn(&str) -> Option<String>) -> bool {
    env("HERDR_ENV").as_deref() == Some("1")
        && env("HERDR_PANE_ID").is_some_and(|pane| !pane.is_empty())
}

/// Whether this process runs in a Superset terminal.
fn in_superset(env: impl Fn(&str) -> Option<String>) -> bool {
    env("SUPERSET_TERMINAL_ID").is_some_and(|terminal| !terminal.is_empty())
}

/// Whether this process runs in a Rex block. A tmux started from a Rex block copies `REX_BLOCK`
/// into every one of its panes, naming the outer block, but sets its own `TERM_PROGRAM`.
pub fn in_rex(env: impl Fn(&str) -> Option<String>) -> bool {
    env("REX_BLOCK").is_some_and(|block| !block.is_empty())
        && env("TERM_PROGRAM").as_deref() == Some("rex")
}

/// The adapter every pane verb uses: `SWARM_ADAPTER` when set, else `herdr` in a Herdr pane, else
/// `superset` in a Superset terminal, else `rex` in a Rex block, else `tmux`. So a chair in Herdr,
/// Superset, or Rex reaches its panes with no variable of its own to keep set.
pub fn adapter(env: impl Fn(&str) -> Option<String>) -> String {
    env("SWARM_ADAPTER").unwrap_or_else(|| {
        if in_herdr(&env) {
            "herdr".into()
        } else if in_superset(&env) {
            "superset".into()
        } else if in_rex(&env) {
            "rex".into()
        } else {
            "tmux".into()
        }
    })
}

/// The contract for this session, or None outside a visible host. `env` reads one variable.
/// Herdr wins over Superset and the app's tmux host when they match too, because its pane is the
/// one on screen. An explicit `SWARM_ADAPTER` other than `superset` skips Superset, so a chair that
/// names tmux or `tmux-swarm` in a Superset terminal gets that host's contract, as `adapter()` does.
/// Rex follows the same rule after Superset.
pub fn context(provider: &str, env: impl Fn(&str) -> Option<String>) -> Option<String> {
    let set = |name: &str| env(name).is_some_and(|value| !value.is_empty());
    let (chair, worker) = if in_herdr(&env) {
        (HERDR_CONTEXT, HERDR_WORKER_CONTEXT)
    } else if in_superset(&env) && env("SWARM_ADAPTER").is_none_or(|name| name == "superset") {
        (SUPERSET_CONTEXT, SUPERSET_WORKER_CONTEXT)
    } else if in_rex(&env) && env("SWARM_ADAPTER").is_none_or(|name| name == "rex") {
        (REX_CONTEXT, REX_WORKER_CONTEXT)
    } else if env("SWARM_ADAPTER").as_deref() == Some("tmux-swarm")
        && set("SWARM_SESSION_ID")
        && (set("TMUX_PANE") || set("SWARM_PTY_PANE"))
    {
        (APP_CONTEXT, APP_WORKER_CONTEXT)
    } else {
        return None;
    };
    let mut context = match is_worker(&env) {
        true => worker.to_string(),
        false => chair.to_string(),
    };
    // A Herdr child pane can be a worker with no swarm id; only a swarm seat gets this block.
    // `SWARM_ROLE` is no pane variable: the caller reads it from the store.
    if let Some(agent) =
        env("SWARM_AGENT_ID").filter(|agent| !agent.is_empty() && agent != "orchestrator")
        && let Some(session) = env("SWARM_SESSION_ID").filter(|session| !session.is_empty())
    {
        let role = env("SWARM_ROLE")
            .map(|role| format!(" with role `{role}`"))
            .unwrap_or_default();
        context.push_str(&format!("\n\n[swarm-seat]\nYou are seat `{agent}`{role} in swarm session `{session}`. Your chair is `orchestrator`.\nSend your final result to the chair with `swarm finish`, and a question with `swarm send orchestrator <kind>`, as the swarm-voice skill says."));
    }
    // pi has no runtime adapter skill, so it gets only the host contract.
    let adapter = match provider {
        "claude" => "~/.swarm/kit/skills/orchestrate-claude/SKILL.md",
        "codex" => "~/.swarm/kit/skills/orchestrate-codex/SKILL.md",
        "agy" => "~/.swarm/kit/skills/orchestrate-agy/SKILL.md",
        _ => return Some(context),
    };
    context.push_str(&format!("\n\n[agent-runtime: {provider}]\nBefore executing any portable skill that requests workers, load `{adapter}`.\nThe runtime adapter translates workflow requirements; the active agent-host contract owns worker lifecycle and visibility."));
    Some(context)
}

/// True in a pane an orchestrator spawned: `swarm spawn` names every child but the orchestrator.
pub fn is_worker(env: impl Fn(&str) -> Option<String>) -> bool {
    env("HERDR_AGENT_PANE").as_deref() == Some("1")
        || env("SWARM_AGENT_ID").is_some_and(|agent| agent != "orchestrator")
}

/// The hook output each provider reads. Codex takes only `additionalContext` into the
/// conversation (`systemMessage` goes to the user), and prints `{}` outside a host.
pub fn render(provider: &str, context: Option<&str>) -> String {
    match (provider, context) {
        ("claude", Some(context)) => format!("{context}\n"),
        ("claude", None) => String::new(),
        ("codex", Some(context)) => serde_json::json!({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": context}}).to_string() + "\n",
        ("codex", None) => "{}\n".into(),
        (_, context) => serde_json::json!({"injectSteps": context.map(|context| vec![serde_json::json!({"ephemeralMessage": context})]).unwrap_or_default()}).to_string() + "\n",
    }
}

/// An AGY worker's history goes through the classifier before the session starts. Best effort:
/// a missing classifier or one that outlives six seconds changes nothing.
pub fn contain_worker_history(provider: &str, payload: &str, home: &str) {
    if provider != "agy" || !is_worker(|name| std::env::var(name).ok()) {
        return;
    }
    let classifier = format!("{home}/.claude/hooks/worker-history-classifier.py");
    let Ok(mut child) = std::process::Command::new(classifier)
        .args(["--provider", provider])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.as_bytes());
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while matches!(child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}
