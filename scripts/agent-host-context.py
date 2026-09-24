#!/usr/bin/env python3

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


HERDR_CONTEXT = """[agent-host: herdr]
This session is running inside Herdr. The top-level session is the orchestrator.
- Every worker must be a visible foreground pane split from HERDR_PANE_ID.
- Set `SWARM_ADAPTER=herdr`. Open one session per run with `swarm session new lane`, export `SWARM_SESSION_ID` and `SWARM_AGENT_ID=orchestrator`, then `swarm agent add orchestrator orchestrator`.
- Spawn with `swarm launch <unique-name> ROLE --cwd "$PWD" [-- extra agent flags]`; it resolves the provider, model and effort, prepares trust, and opens the pane.
- Send work with `swarm send <name> ask`; a reply arrives as the prompt `swarm: new message`, then `swarm inbox`, read, `swarm ack`. Close with `swarm close <name>`.
- Do not use provider-native subagents, headless CLIs, detached processes, or background workers.
- For pane lifecycle detail, load `~/.claude/skills/swarm-orchestrator/SKILL.md`.
- Workers are leaves: answer only, do not orchestrate, spawn descendants, or notify the user."""

HERDR_WORKER_CONTEXT = """[agent-host: herdr — worker]
This session is a worker pane, a child of the orchestrator session. Act only on the task you were assigned.
- Never spawn visible panes: `swarm launch`, `swarm spawn` and every `herdr` surface-creating command (`pane split`, `pane run`, `agent start`, `tab create`, `workspace create`, `worktree create`) are orchestrator-only and are refused for worker sessions.
- Remain a leaf: no provider-native subagents, workflow fan-out, headless one-shots, review rounds, or multi-agent pipelines.
- Report results to the orchestrator only; do not notify the user."""

# Codex-only delta: the native collab tools are a one-token affordance the shared contract
# does not name, and a task that asks for parallel agents reads as permission to use them.
CODEX_HERDR_DELEGATION = """- `spawn_agent`, `wait_agent` and the rest of the codex collaboration family are not a delegation path here: their workers run in-process and Herdr cannot show them. That holds however explicitly a task asks for sub-agents or parallel work — when you cannot create a visible pane, report that and stop; never substitute a hidden worker."""

# Probe delta, for a session with NO drop box only. A provisioned session already knows its
# spawn path, and telling it to probe would manufacture the denial HL-051 exists to remove.
CODEX_SOCKET_PROBE = """- The `swarm launch` line above states the general contract; these two cases resolve it for you and supersede it. Settle your spawn path ONCE, by testing the control socket with a single `herdr status`, then stay on the answer:
  - `herdr status` answers → you are the unsandboxed driver. Spawn with `swarm launch <unique-name> ROLE --cwd "$PWD" [-- extra agent flags]`.
  - `herdr status` answers `PermissionDenied` → your sandbox denies that socket, so every direct `swarm` or `herdr` call is denied for the same reason. You have no spawn path: report that and stop. Do not retry the call, and do not request an escalated sandbox to force one through — escalation spends a user approval on a path this session is not meant to use."""

RUNTIME_ADAPTERS = {
    "claude": "~/.claude/skills/orchestrate-claude/SKILL.md",
    "codex": "~/.agents/skills/orchestrate-codex/SKILL.md",
    "agy": "~/.gemini/config/skills/orchestrate-agy/SKILL.md",
}


WORKER_HISTORY = Path("~/.claude/hooks/worker-history-classifier.py").expanduser()


def worker_session():
    """True in a pane an orchestrator spawned. `swarm spawn` names every child but the
    orchestrator, so the name the pane was given IS the marker."""
    agent = os.environ.get("SWARM_AGENT_ID")
    return (os.environ.get("HERDR_AGENT_PANE") == "1"
            or (agent is not None and agent != "orchestrator"))


def active_context():
    worker = worker_session()
    if os.environ.get("HERDR_ENV") == "1" and os.environ.get("HERDR_PANE_ID"):
        return HERDR_WORKER_CONTEXT if worker else HERDR_CONTEXT
    return None


def session_start_output(context):
    """Codex SessionStart hook output, or `{}` when no visible host is active.

    `additionalContext` is the only field of this hook the model actually receives;
    `systemMessage` surfaces to the user instead. That is why codex root 019faa65 ran 19
    invisible `spawn_agent` workers while this hook was installed and trusted — the
    contract was emitted, just never into the conversation.
    """
    if not context:
        return {}
    return {
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    }


def bind_runtime(context, provider):
    if not context:
        return context
    adapter = RUNTIME_ADAPTERS[provider]
    return f"""{context}

[agent-runtime: {provider}]
Before executing any portable skill that requests workers, load `{adapter}`.
The runtime adapter translates workflow requirements; the active agent-host contract owns worker lifecycle and visibility."""


def contain_worker_history(provider, payload, runner=subprocess.run):
    if provider != "agy" or not worker_session():
        return
    try:
        runner(
            [str(WORKER_HISTORY), "--provider", provider],
            input=payload,
            text=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=6,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--provider", choices=("claude", "codex", "agy"), required=True)
    args = parser.parse_args()
    payload = sys.stdin.read()
    contain_worker_history(args.provider, payload)
    context = active_context()
    if args.provider == "codex" and context == HERDR_CONTEXT:
        context = f"{context}\n{CODEX_HERDR_DELEGATION}\n{CODEX_SOCKET_PROBE}"
    context = bind_runtime(context, args.provider)
    if args.provider == "claude":
        if context:
            print(context)
    elif args.provider == "codex":
        print(json.dumps(session_start_output(context)))
    else:
        steps = [{"ephemeralMessage": context}] if context else []
        print(json.dumps({"injectSteps": steps}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
