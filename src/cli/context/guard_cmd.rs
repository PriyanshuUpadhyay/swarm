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
            let (payload, owner_bash_rules) = match payload {
                Some(payload) if provider == Provider::Pi => match pi_process_payload(payload) {
                    Ok(call) => call,
                    Err(reason) => {
                        return swarm::guard::render(
                            provider,
                            &swarm::guard::Verdict::Deny(reason),
                        );
                    }
                },
                other => (other, false),
            };
            let wall = payload
                .as_ref()
                .and_then(|payload| swarm::guard::wall(provider, event, payload));
            let verdict = match (std::fs::read_to_string(path.as_ref()), payload) {
                (_, None) => swarm::guard::Verdict::Deny(
                    "swarm guard: cannot read the hook payload on stdin within 2 s, so the call is blocked".into(),
                ),
                (Err(error), _)
                    if error.kind() != std::io::ErrorKind::NotFound
                        && (provider != Provider::Pi || owner_bash_rules) =>
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
                // In panes the owner's extension checks Bash. A native child also goes
                // through the registered rules, independent of the owner's Pi extension.
                (_, Some(_)) if provider == Provider::Pi && !owner_bash_rules => swarm::guard::Verdict::Allow,
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

/// Treat a process start as bash for both the Swarm walls and the configured rules.
fn pi_process_payload(payload: String) -> Result<(Option<String>, bool), String> {
    let Ok(mut call) = serde_json::from_str::<serde_json::Value>(&payload) else {
        return Err("swarm guard: invalid Pi tool payload".into());
    };
    if call["tool_name"] != "process" {
        let child_bash =
            call["tool_name"] == "bash" && env::var_os("SWARM_PI_NATIVE_CHILD").is_some();
        return Ok((Some(payload), child_bash));
    }
    match call["tool_input"]["action"].as_str() {
        Some("start") => {
            call["tool_input"]["command"]
                .as_str()
                .filter(|command| !command.trim().is_empty())
                .ok_or("swarm guard: cannot check a process start without its command")?;
            // The targeted process may run a shell in a folder other than Pi's working folder.
            if let (Some(base), Some(cwd)) = (call["cwd"].as_str(), call["tool_input"]["cwd"].as_str()) {
                let cwd = swarm::task::resolve(std::path::Path::new(base), std::path::Path::new(cwd));
                call["cwd"] = cwd.to_string_lossy().into_owned().into();
                call["tool_input"].as_object_mut().unwrap().remove("cwd");
            }
            call["tool_name"] = "bash".into();
            Ok((Some(call.to_string()), true))
        }
        Some("list" | "stop" | "output" | "update" | "clear") => Ok((Some(payload), false)),
        // Input may be executable by an already running shell. Its context is not a shell line
        // we can check here, so refuse model-written input rather than treating it as data.
        Some("write") if call["tool_input"]["input"].as_str().unwrap_or("").is_empty() => Ok((Some(payload), false)),
        Some("write") => Err("swarm guard: cannot check commands sent to a running process; model-written stdin is blocked".into()),
        _ => Err("swarm guard: unknown Pi process action".into()),
    }
}

fn bridged_tool(payload: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload).is_ok_and(|payload| {
        payload["tool_name"]
            .as_str()
            .is_some_and(|name| name.starts_with("mcp__"))
    })
}
