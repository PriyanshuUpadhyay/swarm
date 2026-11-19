//! `swarm guard`, the tool-call hook every agent CLI runs (ADR 0040).

use super::*;

/// `swarm guard <provider> <event>`: the rule list's verdict on one tool call, in the CLI's own
/// reply format (ADR 0040). It never opens the store, so a busy bus cannot slow a tool call.
pub(crate) fn guard(args: &[String]) -> (String, String, i32) {
    let [provider, event] = args else {
        return (String::new(), format!("{USAGE}\n"), 2);
    };
    let Some(provider) = Provider::parse(provider) else {
        return (String::new(), format!("{USAGE}\n"), 2);
    };
    let deadline = std::time::Instant::now() + swarm::guard::DEADLINE;
    let verdict = match (swarm::paths::guards_file(), env::var("HOME")) {
        (Ok(path), Ok(home)) => {
            let path = path.to_string_lossy();
            let payload = read_within(std::io::stdin(), std::time::Duration::from_secs(2));
            // In a turn of Pi's Claude bridge, Claude Code has only Pi's tools, as `mcp__` tools,
            // and Pi runs each one through its own guards. A native tool of a Claude Code that Pi
            // started, such as AskClaude's, still gets the rules below.
            if provider == Provider::Claude
                && swarm::host::run_by_pi(|name| env::var(name).ok())
                && payload.as_deref().is_some_and(bridged_tool)
            {
                return swarm::guard::render(provider, &swarm::guard::Verdict::Allow);
            }
            let wall = payload
                .as_ref()
                .and_then(|payload| swarm::guard::wall(provider, event, payload));
            let verdict = match (std::fs::read_to_string(path.as_ref()), payload) {
                (_, None) => swarm::guard::Verdict::Deny(
                    "swarm guard: cannot read the hook payload on stdin within 2 s, so the call is blocked".into(),
                ),
                (Err(error), _)
                    if error.kind() != std::io::ErrorKind::NotFound && provider != Provider::Pi =>
                {
                    swarm::guard::Verdict::Deny(format!(
                        "swarm guard: cannot read {path}: {error}, so the call is blocked"
                    ))
                }
                (_, Some(_))
                    if let Some(swarm::guard::Verdict::Deny(reason)) = &wall =>
                {
                    swarm::guard::Verdict::Deny(reason.clone())
                }
                (_, Some(payload))
                    if swarm::host::is_worker(|name| env::var(name).ok())
                        && let Some(wall) = swarm::guard::board_wall(provider, event, &payload) =>
                {
                    wall
                }
                // Swarm's pi extension calls the guard for swarm's own walls only; the owner's
                // pi extensions run the owner's rules (ADR 0086).
                (_, Some(_)) if provider == Provider::Pi => swarm::guard::Verdict::Allow,
                (list, Some(payload)) => swarm::guard::decide(
                    list.ok().as_deref(),
                    &path,
                    provider,
                    event,
                    &payload,
                    &home,
                    deadline,
                ),
            };
            // A warning never skips the worker wall or the owner's rules.
            if verdict == swarm::guard::Verdict::Allow {
                wall.unwrap_or(verdict)
            } else {
                verdict
            }
        }
        _ => swarm::guard::Verdict::Deny(
            "swarm guard: HOME is not set, so the call is blocked".into(),
        ),
    };
    swarm::guard::render(provider, &verdict)
}

fn bridged_tool(payload: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload).is_ok_and(|payload| {
        payload["tool_name"]
            .as_str()
            .is_some_and(|name| name.starts_with("mcp__"))
    })
}
