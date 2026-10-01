//! The host contract a new agent session reads at start: which host it runs in, whether it is the
//! orchestrator or a worker, and which runtime adapter skill it loads before it spawns anything.
//! Claude, Codex, and AGY run `swarm host-context --provider <p>` as a SessionStart hook.

use std::io::Write;

const HERDR_CONTEXT: &str = r#"[agent-host: herdr]
This session is running inside Herdr. The top-level session is the orchestrator.
- A pane worker is a visible foreground pane split from this pane.
- Open one session per run with `swarm session new lane`; it registers this pane as the orchestrator.
- Spawn with `swarm launch <unique-name> ROLE --cwd "$PWD" [-- extra agent flags]`; it resolves the provider, model and effort, prepares trust, and opens the pane.
- Send work with `swarm send <name> ask`; a reply arrives as the prompt `swarm: new message`, then `swarm inbox`, read, `swarm ack`. Close with `swarm close <name>`.
- Native background subagents are allowed. Prefer a visible pane when the user must watch or answer the worker, when it runs on another provider, or when a skill asks for visible seats.
- Do not use headless CLIs or detached processes.
- For pane lifecycle detail, load `~/.claude/skills/swarm-orchestrator/SKILL.md`.
- Workers are leaves: answer only, do not orchestrate, spawn descendants, or notify the user."#;

const HERDR_WORKER_CONTEXT: &str = r#"[agent-host: herdr — worker]
This session is a worker pane, a child of the orchestrator session. Act only on the task you were assigned.
- Never spawn visible panes: `swarm launch`, `swarm spawn` and every `herdr` surface-creating command (`pane split`, `pane run`, `agent start`, `tab create`, `workspace create`, `worktree create`) are orchestrator-only and are refused for worker sessions.
- Remain a leaf: no provider-native subagents, workflow fan-out, headless one-shots, review rounds, or multi-agent pipelines.
- Report results to the orchestrator only; do not notify the user."#;

// The app opens the session and launches the chair as `orchestrator` before the CLI starts, so
// the chair must not open a second session that the app would not show.
const APP_CONTEXT: &str = r#"[agent-host: swarm-app]
This session is a chair that the Swarm app started. The top-level session is the orchestrator.
- The swarm session is already open and this chair is registered as `orchestrator`. Do not run `swarm session new` or `swarm agent add`.
- A pane worker is a child agent CLI in its own tmux session; the app shows each child as a visible column beside the chat.
- Spawn with `swarm launch <unique-name> ROLE --cwd "$PWD" [-- extra agent flags]`; it resolves the provider, model and effort, prepares trust, and opens the pane.
- Send work with `swarm send <name> ask`; a reply arrives as the prompt `swarm: new message`, then `swarm inbox`, read, `swarm ack`. Close with `swarm close <name>`.
- Native background subagents are allowed. Prefer a visible pane when the user must watch or answer the worker, when it runs on another provider, or when a skill asks for visible seats.
- Do not use headless CLIs or detached processes.
- For pane lifecycle detail, load `~/.claude/skills/swarm-orchestrator/SKILL.md` and skip its Session section.
- Workers are leaves: answer only, do not orchestrate, spawn descendants, or notify the user."#;

const APP_WORKER_CONTEXT: &str = r#"[agent-host: swarm-app — worker]
This session is a worker pane, a child of the orchestrator session. Act only on the task you were assigned.
- Never spawn panes: `swarm launch` and `swarm spawn` are orchestrator-only and are refused for worker sessions.
- Remain a leaf: no provider-native subagents, workflow fan-out, headless one-shots, review rounds, or multi-agent pipelines.
- Report results to the orchestrator only; do not notify the user."#;

/// Whether this process runs in a Herdr pane.
fn in_herdr(env: impl Fn(&str) -> Option<String>) -> bool {
    env("HERDR_ENV").as_deref() == Some("1")
        && env("HERDR_PANE_ID").is_some_and(|pane| !pane.is_empty())
}

/// The adapter every pane verb uses: `SWARM_ADAPTER` when set, else `herdr` in a Herdr pane, else
/// `tmux`. So a chair in Herdr reaches its panes with no variable of its own to keep set.
pub fn adapter(env: impl Fn(&str) -> Option<String>) -> String {
    env("SWARM_ADAPTER").unwrap_or_else(|| match in_herdr(&env) {
        true => "herdr".into(),
        false => "tmux".into(),
    })
}

/// The contract for this session, or None outside a visible host. `env` reads one variable.
/// Herdr wins over the app's tmux host when both match, because its pane is the one on screen.
pub fn context(provider: &str, env: impl Fn(&str) -> Option<String>) -> Option<String> {
    let set = |name: &str| env(name).is_some_and(|value| !value.is_empty());
    let (chair, worker) = if in_herdr(&env) {
        (HERDR_CONTEXT, HERDR_WORKER_CONTEXT)
    } else if env("SWARM_ADAPTER").as_deref() == Some("tmux-solo")
        && set("SWARM_SESSION_ID")
        && set("TMUX_PANE")
    {
        (APP_CONTEXT, APP_WORKER_CONTEXT)
    } else {
        return None;
    };
    let mut context = match is_worker(&env) {
        true => worker.to_string(),
        false => chair.to_string(),
    };
    let adapter = match provider {
        "claude" => "~/.claude/skills/orchestrate-claude/SKILL.md",
        "codex" => "~/.agents/skills/orchestrate-codex/SKILL.md",
        _ => "~/.gemini/config/skills/orchestrate-agy/SKILL.md",
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

/// The agent state one provider hook event reports, with its detail, or None when the event says
/// nothing about state. `swarm hook` calls this; see ADR 0021 for the table. AGY puts no event
/// name in its payload, so the caller passes it; Claude and Codex send `hook_event_name`.
pub fn hook_state(
    provider: &str,
    event: &str,
    payload: &serde_json::Value,
) -> Option<(&'static str, Option<String>)> {
    let text = |name: &str| payload.get(name).and_then(serde_json::Value::as_str);
    let error = text("error").filter(|error| !error.is_empty());
    let state = match (provider, event) {
        // An in-process Claude subagent's events are not the pane's state.
        ("claude", _) if text("agent_id").is_some_and(|id| !id.is_empty()) => return None,
        ("claude" | "codex", "UserPromptSubmit" | "PreToolUse" | "PostToolUse") => "working",
        ("claude" | "codex", "PermissionRequest") | ("claude", "Elicitation") => "waiting",
        ("claude" | "codex", "Stop") | ("codex", "Interrupt") => "done",
        ("claude", "StopFailure") => return Some(("failed", error.map(str::to_string))),
        ("claude", "Notification") => match text("notification_type")? {
            "permission_prompt" | "elicitation_dialog" | "agent_needs_input" => "waiting",
            "idle_prompt" => "done",
            _ => return None,
        },
        ("agy", "PreInvocation" | "PostToolUse") => "working",
        ("agy", "Stop") => match error {
            Some(error) => return Some(("failed", Some(error.to_string()))),
            None => "done",
        },
        _ => return None,
    };
    Some((state, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    const HERDR: [(&str, &str); 2] = [("HERDR_ENV", "1"), ("HERDR_PANE_ID", "wK:p1")];

    #[test]
    fn a_herdr_pane_picks_herdr_unless_swarm_adapter_names_another() {
        assert_eq!(adapter(env(&HERDR)), "herdr");
        assert_eq!(adapter(env(&[])), "tmux");
        assert_eq!(adapter(env(&[HERDR[0], ("HERDR_PANE_ID", "")])), "tmux");
        assert_eq!(
            adapter(env(&[HERDR[0], HERDR[1], ("SWARM_ADAPTER", "tmux-solo")])),
            "tmux-solo"
        );
    }

    #[test]
    fn outside_a_host_codex_gets_an_empty_object_and_claude_nothing() {
        assert_eq!(
            render("codex", context("codex", env(&[])).as_deref()),
            "{}\n"
        );
        assert_eq!(render("claude", context("claude", env(&[])).as_deref()), "");
        assert_eq!(render("agy", None), "{\"injectSteps\":[]}\n");
    }

    #[test]
    fn codex_and_claude_orchestrators_get_the_same_host_contract() {
        let codex = context("codex", env(&HERDR)).unwrap();
        let claude = context("claude", env(&HERDR)).unwrap();
        assert!(codex.find("[agent-host: herdr]") < codex.find("[agent-runtime: codex]"));
        assert_eq!(
            codex.split("\n\n[agent-runtime").next(),
            claude.split("\n\n[agent-runtime").next()
        );
        assert!(!codex.contains("herdr status"));
    }

    #[test]
    fn a_worker_marker_gives_the_worker_contract_but_the_orchestrator_seat_does_not() {
        for marker in [("HERDR_AGENT_PANE", "1"), ("SWARM_AGENT_ID", "cl-seat-1")] {
            let body = context("codex", env(&[HERDR[0], HERDR[1], marker])).unwrap();
            assert!(body.contains("[agent-host: herdr — worker]"), "{marker:?}");
        }
        let seat = context(
            "claude",
            env(&[HERDR[0], HERDR[1], ("SWARM_AGENT_ID", "orchestrator")]),
        )
        .unwrap();
        assert!(seat.starts_with("[agent-host: herdr]\n"));
    }

    const APP_CHAIR: [(&str, &str); 4] = [
        ("SWARM_ADAPTER", "tmux-solo"),
        ("SWARM_SESSION_ID", "01a0eda3-3d95-7f40-a754-77b47626e7ee"),
        ("SWARM_AGENT_ID", "orchestrator"),
        ("TMUX_PANE", "%0"),
    ];

    #[test]
    fn an_app_chair_gets_the_app_contract_with_its_runtime() {
        for provider in ["codex", "claude"] {
            let chair = context(provider, env(&APP_CHAIR)).unwrap();
            assert!(chair.starts_with("[agent-host: swarm-app]\n"), "{provider}");
            assert!(chair.contains("Do not run `swarm session new`"));
            assert!(chair.contains(&format!("\n\n[agent-runtime: {provider}]\n")));
        }
    }

    #[test]
    fn a_child_of_an_app_chair_gets_the_app_worker_contract() {
        let mut seat = APP_CHAIR;
        seat[2] = ("SWARM_AGENT_ID", "cl-seat-1");
        let body = context("codex", env(&seat)).unwrap();
        assert!(body.starts_with("[agent-host: swarm-app — worker]\n"));
    }

    #[test]
    fn the_app_contract_needs_the_solo_adapter_a_session_and_a_tmux_pane() {
        for index in 0..APP_CHAIR.len() {
            if APP_CHAIR[index].0 == "SWARM_AGENT_ID" {
                continue;
            }
            let mut partial = APP_CHAIR;
            partial[index].1 = "";
            assert_eq!(
                context("codex", env(&partial)),
                None,
                "{:?}",
                APP_CHAIR[index]
            );
        }
        let mut plain_tmux = APP_CHAIR;
        plain_tmux[0] = ("SWARM_ADAPTER", "tmux");
        assert_eq!(context("codex", env(&plain_tmux)), None);
    }

    #[test]
    fn herdr_wins_when_an_app_chair_env_is_also_present() {
        let both: Vec<_> = HERDR.iter().chain(APP_CHAIR.iter()).copied().collect();
        let body = context("claude", env(&both)).unwrap();
        assert!(body.starts_with("[agent-host: herdr]\n"));
    }

    #[test]
    fn maps_every_hook_event_in_the_state_table() {
        use serde_json::json;
        let empty = json!({});
        let notification = |kind: &str| json!({"notification_type": kind});
        type Expected<'a> = Option<(&'a str, Option<&'a str>)>;
        let cases: &[(&str, &str, serde_json::Value, Expected)] = &[
            (
                "claude",
                "UserPromptSubmit",
                empty.clone(),
                Some(("working", None)),
            ),
            (
                "claude",
                "PreToolUse",
                empty.clone(),
                Some(("working", None)),
            ),
            (
                "claude",
                "PostToolUse",
                empty.clone(),
                Some(("working", None)),
            ),
            (
                "claude",
                "PermissionRequest",
                empty.clone(),
                Some(("waiting", None)),
            ),
            (
                "claude",
                "Elicitation",
                empty.clone(),
                Some(("waiting", None)),
            ),
            (
                "claude",
                "Notification",
                notification("permission_prompt"),
                Some(("waiting", None)),
            ),
            (
                "claude",
                "Notification",
                notification("elicitation_dialog"),
                Some(("waiting", None)),
            ),
            (
                "claude",
                "Notification",
                notification("agent_needs_input"),
                Some(("waiting", None)),
            ),
            (
                "claude",
                "Notification",
                notification("idle_prompt"),
                Some(("done", None)),
            ),
            ("claude", "Notification", notification("auth_success"), None),
            ("claude", "Notification", empty.clone(), None),
            ("claude", "Stop", empty.clone(), Some(("done", None))),
            (
                "claude",
                "StopFailure",
                json!({"error": "rate_limit"}),
                Some(("failed", Some("rate_limit"))),
            ),
            (
                "claude",
                "StopFailure",
                empty.clone(),
                Some(("failed", None)),
            ),
            (
                "claude",
                "PreToolUse",
                json!({"agent_id": "explorer"}),
                None,
            ),
            (
                "claude",
                "Stop",
                json!({"agent_id": ""}),
                Some(("done", None)),
            ),
            ("claude", "SessionStart", empty.clone(), None),
            ("claude", "Interrupt", empty.clone(), None),
            (
                "codex",
                "UserPromptSubmit",
                empty.clone(),
                Some(("working", None)),
            ),
            (
                "codex",
                "PreToolUse",
                empty.clone(),
                Some(("working", None)),
            ),
            (
                "codex",
                "PostToolUse",
                empty.clone(),
                Some(("working", None)),
            ),
            (
                "codex",
                "PermissionRequest",
                empty.clone(),
                Some(("waiting", None)),
            ),
            ("codex", "Stop", empty.clone(), Some(("done", None))),
            ("codex", "Interrupt", empty.clone(), Some(("done", None))),
            ("codex", "StopFailure", empty.clone(), None),
            ("codex", "SessionStart", empty.clone(), None),
            (
                "agy",
                "PreInvocation",
                empty.clone(),
                Some(("working", None)),
            ),
            ("agy", "PostToolUse", empty.clone(), Some(("working", None))),
            ("agy", "Stop", empty.clone(), Some(("done", None))),
            ("agy", "Stop", json!({"error": ""}), Some(("done", None))),
            (
                "agy",
                "Stop",
                json!({"error": "quota"}),
                Some(("failed", Some("quota"))),
            ),
            ("agy", "PreToolUse", empty.clone(), None),
            ("gemini", "Stop", empty.clone(), None),
        ];
        for (provider, event, payload, expected) in cases {
            let got = hook_state(provider, event, payload);
            let got = got
                .as_ref()
                .map(|(state, detail)| (*state, detail.as_deref()));
            assert_eq!(got, *expected, "{provider} {event} {payload}");
        }
    }
}
