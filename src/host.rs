//! The host contract a new agent session reads at start: which host it runs in, whether it is the
//! orchestrator or a worker, and which runtime adapter skill it loads before it spawns anything.
//! Claude, Codex, and AGY run `swarm host-context --provider <p>` as a SessionStart hook.

const HERDR_CONTEXT: &str = r#"[agent-host: herdr]
This session is running inside Herdr. The top-level session is the orchestrator.
- A pane worker is a visible foreground pane split from this pane.
- Open one session per run with `swarm session new lane`; it registers this pane as the orchestrator.
- Spawn with `swarm launch <unique-name> ROLE --cwd "$PWD" [-- extra agent flags]`; it resolves the provider, model and effort, prepares trust, and opens the pane.
- Send work with `swarm send <name> ask`; a reply arrives as the prompt `swarm: new message`, then `swarm inbox`, read, `swarm ack`. Close with `swarm close <name>`.
- Native background subagents are allowed. Prefer a visible pane when the user must watch or answer the worker, when it runs on another provider, or when a skill asks for visible seats.
- Do not use headless CLIs or detached processes.
- For pane lifecycle detail, load `~/.swarm/kit/skills/swarm-orchestrator/SKILL.md`.
- When the run is done, notify the owner with `swarm notify "swarm: <project> done" --body "<one line>"`; swarm itself notifies when an agent waits on a permission or a question.
- Workers are leaves: answer only, do not orchestrate, spawn descendants, or notify the user."#;

const HERDR_WORKER_CONTEXT: &str = r#"[agent-host: herdr — worker]
This session is a worker pane, a child of the orchestrator session. Act only on the task you were assigned.
- Never spawn visible panes: `swarm launch`, `swarm spawn` and every `herdr` surface-creating command (`pane split`, `pane run`, `agent start`, `tab create`, `workspace create`, `worktree create`) are orchestrator-only and are refused for worker sessions.
- Remain a leaf: no provider-native subagents, workflow fan-out, headless one-shots, review rounds, or multi-agent pipelines.
- Report results to the orchestrator only; do not notify the user."#;

const SUPERSET_CONTEXT: &str = r#"[agent-host: superset]
This session is running inside a Superset terminal. The top-level session is the orchestrator.
- A pane worker is a Superset terminal in the same workspace, created beside this one.
- Open one session per run with `swarm session new lane`; it registers this terminal as the orchestrator.
- Spawn with `swarm launch <unique-name> ROLE --cwd "$PWD" [-- extra agent flags]`; it resolves the provider, model and effort, prepares trust, and opens the pane.
- Send work with `swarm send <name> ask`; a reply arrives as the prompt `swarm: new message`, then `swarm inbox`, read, `swarm ack`. Close with `swarm close <name>`.
- `swarm answer` and `swarm interrupt` are not available on this host; answer a worker's prompt in the Superset app.
- Native background subagents are allowed. Prefer a visible pane when the user must watch or answer the worker, when it runs on another provider, or when a skill asks for visible seats.
- Do not use headless CLIs or detached processes.
- For pane lifecycle detail, load `~/.swarm/kit/skills/swarm-orchestrator/SKILL.md`.
- When the run is done, notify the owner with `swarm notify "swarm: <project> done" --body "<one line>"`; swarm itself notifies when an agent waits on a permission or a question.
- Workers are leaves: answer only, do not orchestrate, spawn descendants, or notify the user."#;

const SUPERSET_WORKER_CONTEXT: &str = r#"[agent-host: superset — worker]
This session is a worker pane, a child of the orchestrator session. Act only on the task you were assigned.
- Never spawn panes: `swarm launch` and `swarm spawn` are orchestrator-only and are refused for worker sessions.
- Remain a leaf: no provider-native subagents, workflow fan-out, headless one-shots, review rounds, or multi-agent pipelines.
- Report results to the orchestrator only; do not notify the user."#;

const REX_CONTEXT: &str = r#"[agent-host: rex]
This session is running inside a Rex block. The top-level session is the orchestrator.
- A pane worker is a Rex block in the same session; the first one splits right of this block and the rest stack below it.
- Open one session per run with `swarm session new lane`; it registers this block as the orchestrator.
- Spawn with `swarm launch <unique-name> ROLE --cwd "$PWD" [-- extra agent flags]`; it resolves the provider, model and effort, prepares trust, and opens the pane.
- Send work with `swarm send <name> ask`; a reply arrives as the prompt `swarm: new message`, then `swarm inbox`, read, `swarm ack`. Close with `swarm close <name>`.
- Native background subagents are allowed. Prefer a visible pane when the user must watch or answer the worker, when it runs on another provider, or when a skill asks for visible seats.
- Do not use headless CLIs or detached processes.
- For pane lifecycle detail, load `~/.swarm/kit/skills/swarm-orchestrator/SKILL.md`.
- When the run is done, notify the owner with `swarm notify "swarm: <project> done" --body "<one line>"`; swarm itself notifies when an agent waits on a permission or a question.
- Workers are leaves: answer only, do not orchestrate, spawn descendants, or notify the user."#;

const REX_WORKER_CONTEXT: &str = r#"[agent-host: rex — worker]
This session is a worker pane, a child of the orchestrator session. Act only on the task you were assigned.
- Never spawn panes: `swarm launch`, `swarm spawn`, and Rex block-creating commands (`rex split`, `rex run`, `rex new`) are orchestrator-only.
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
- For pane lifecycle detail, load `~/.swarm/kit/skills/swarm-orchestrator/SKILL.md` and skip its Session section.
- When the run is done, notify the owner with `swarm notify "swarm: <project> done" --body "<one line>"`; swarm itself notifies when an agent waits on a permission or a question.
- Workers are leaves: answer only, do not orchestrate, spawn descendants, or notify the user."#;

const APP_WORKER_CONTEXT: &str = r#"[agent-host: swarm-app — worker]
This session is a worker pane, a child of the orchestrator session. Act only on the task you were assigned.
- Never spawn panes: `swarm launch` and `swarm spawn` are orchestrator-only and are refused for worker sessions.
- Remain a leaf: no provider-native subagents, workflow fan-out, headless one-shots, review rounds, or multi-agent pipelines.
- Report results to the orchestrator only; do not notify the user."#;

mod context;
mod status;
mod usage;

pub use context::{adapter, contain_worker_history, context, in_rex, is_worker, render};
pub use status::{
    HookState, StatusScanner, hook_state, host_record, program_status, program_status_report,
};
pub use usage::{
    COUNTED_MESSAGES, ClaudeLogUsage, ClaudeUsage, UsageSnapshot, UsageState, claude_usage,
    codex_usage_snapshot,
};

#[cfg(test)]
#[path = "host/status_tests.rs"]
mod status_tests;

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
            adapter(env(&[HERDR[0], HERDR[1], ("SWARM_ADAPTER", "tmux-swarm")])),
            "tmux-swarm"
        );
    }

    const SUPERSET: [(&str, &str); 1] = [("SUPERSET_TERMINAL_ID", "t1")];

    #[test]
    fn a_superset_terminal_picks_superset_unless_herdr_or_swarm_adapter_wins() {
        assert_eq!(adapter(env(&SUPERSET)), "superset");
        assert_eq!(adapter(env(&[("SUPERSET_TERMINAL_ID", "")])), "tmux");
        assert_eq!(adapter(env(&[HERDR[0], HERDR[1], SUPERSET[0]])), "herdr");
        assert_eq!(
            adapter(env(&[SUPERSET[0], ("SWARM_ADAPTER", "tmux")])),
            "tmux"
        );
    }

    #[test]
    fn the_superset_contract_marks_chair_and_worker() {
        let chair = context("claude", env(&SUPERSET)).unwrap();
        assert!(chair.starts_with("[agent-host: superset]\n"));
        assert!(chair.contains("`swarm answer` and `swarm interrupt` are not available"));
        let worker = context(
            "claude",
            env(&[SUPERSET[0], ("SWARM_AGENT_ID", "cl-seat-1")]),
        );
        assert!(
            worker
                .unwrap()
                .starts_with("[agent-host: superset — worker]")
        );
        let herdr = context("claude", env(&[HERDR[0], HERDR[1], SUPERSET[0]])).unwrap();
        assert!(herdr.starts_with("[agent-host: herdr]\n"));
    }

    const REX: [(&str, &str); 2] = [("REX_BLOCK", "block:1"), ("TERM_PROGRAM", "rex")];

    #[test]
    fn a_rex_block_picks_rex_unless_herdr_superset_or_swarm_adapter_wins() {
        assert_eq!(adapter(env(&REX)), "rex");
        assert_eq!(adapter(env(&[("REX_BLOCK", ""), REX[1]])), "tmux");
        assert_eq!(adapter(env(&[HERDR[0], HERDR[1], REX[0], REX[1]])), "herdr");
        assert_eq!(adapter(env(&[SUPERSET[0], REX[0], REX[1]])), "superset");
        assert_eq!(
            adapter(env(&[REX[0], REX[1], ("SWARM_ADAPTER", "tmux")])),
            "tmux"
        );
    }

    #[test]
    fn a_tmux_pane_inside_rex_is_not_a_rex_block() {
        let nested = [REX[0], ("TERM_PROGRAM", "tmux"), ("TMUX_PANE", "%0")];
        assert_eq!(adapter(env(&nested)), "tmux");
        assert_eq!(context("claude", env(&nested)), None);
    }

    #[test]
    fn the_rex_contract_marks_chair_and_worker() {
        let chair = context("claude", env(&REX)).unwrap();
        assert!(chair.starts_with("[agent-host: rex]\n"));
        let worker = context(
            "claude",
            env(&[REX[0], REX[1], ("SWARM_AGENT_ID", "cl-seat-1")]),
        );
        assert!(worker.unwrap().starts_with("[agent-host: rex — worker]"));
        let herdr = context("claude", env(&[HERDR[0], HERDR[1], REX[0], REX[1]])).unwrap();
        assert!(herdr.starts_with("[agent-host: herdr]\n"));
        assert_eq!(
            context("claude", env(&[REX[0], REX[1], ("SWARM_ADAPTER", "tmux")])),
            None
        );
        let named = context("claude", env(&[REX[0], REX[1], ("SWARM_ADAPTER", "rex")]));
        assert!(named.unwrap().starts_with("[agent-host: rex]\n"));
    }

    /// A seat learns its own id, role, session, chair, and result rule at start, on every host.
    #[test]
    fn a_worker_contract_names_the_seat_and_where_its_result_goes() {
        let seat = [
            ("SWARM_AGENT_ID", "cl-seat-1"),
            ("SWARM_ROLE", "review.check"),
            ("SWARM_SESSION_ID", "01a15391-109c-75b2-b09c-5eac0d553e25"),
        ];
        for host in [&REX[..], &HERDR[..]] {
            let worker = context("codex", env(&[host, &seat].concat())).unwrap();
            assert!(worker.contains(
                "\n\n[swarm-seat]\nYou are seat `cl-seat-1` with role `review.check` in swarm session `01a15391-109c-75b2-b09c-5eac0d553e25`. Your chair is `orchestrator`.\nSend your final result to the chair with `swarm finish`, and a question with `swarm send orchestrator <kind>`, as the swarm-voice skill says."
            ), "{worker}");
            assert!(worker.find("[swarm-seat]") < worker.find("[agent-runtime: codex]"));
        }
        let no_role = context("claude", env(&[REX[0], REX[1], seat[0], seat[2]])).unwrap();
        assert!(
            no_role.contains("You are seat `cl-seat-1` in swarm session"),
            "{no_role}"
        );
        let chair = context(
            "claude",
            env(&[
                REX[0],
                REX[1],
                ("SWARM_AGENT_ID", "orchestrator"),
                seat[1],
                seat[2],
            ]),
        )
        .unwrap();
        assert!(!chair.contains("[swarm-seat]"));
        let empty = context(
            "claude",
            env(&[REX[0], REX[1], ("SWARM_AGENT_ID", ""), seat[2]]),
        );
        assert!(!empty.unwrap().contains("[swarm-seat]"));
        let herdr_child = context(
            "claude",
            env(&[HERDR[0], HERDR[1], ("HERDR_AGENT_PANE", "1")]),
        );
        assert!(!herdr_child.unwrap().contains("[swarm-seat]"));
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

    /// ADR 0044: the chair notifies the owner when its run is done; a worker never does.
    #[test]
    fn only_the_chair_contracts_name_swarm_notify() {
        let line = "- When the run is done, notify the owner with `swarm notify";
        for chair in [HERDR_CONTEXT, SUPERSET_CONTEXT, REX_CONTEXT, APP_CONTEXT] {
            assert!(chair.contains(line), "{chair}");
        }
        for worker in [
            HERDR_WORKER_CONTEXT,
            SUPERSET_WORKER_CONTEXT,
            REX_WORKER_CONTEXT,
            APP_WORKER_CONTEXT,
        ] {
            assert!(!worker.contains("swarm notify"), "{worker}");
        }
    }

    const APP_CHAIR: [(&str, &str); 4] = [
        ("SWARM_ADAPTER", "tmux-swarm"),
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
    fn an_explicit_swarm_adapter_wins_over_the_superset_contract() {
        assert_eq!(
            context("claude", env(&[SUPERSET[0], ("SWARM_ADAPTER", "tmux")])),
            None
        );
        let app: Vec<_> = APP_CHAIR.iter().chain(SUPERSET.iter()).copied().collect();
        let body = context("claude", env(&app)).unwrap();
        assert!(body.starts_with("[agent-host: swarm-app]\n"));
        let named = context("claude", env(&[SUPERSET[0], ("SWARM_ADAPTER", "superset")]));
        assert!(named.unwrap().starts_with("[agent-host: superset]\n"));
    }

    #[test]
    fn herdr_wins_when_an_app_chair_env_is_also_present() {
        let both: Vec<_> = HERDR.iter().chain(APP_CHAIR.iter()).copied().collect();
        let body = context("claude", env(&both)).unwrap();
        assert!(body.starts_with("[agent-host: herdr]\n"));
    }
}
