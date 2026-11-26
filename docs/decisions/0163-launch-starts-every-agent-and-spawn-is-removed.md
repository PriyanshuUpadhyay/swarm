---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0008", "0123", "0153"]
related: ["0008", "0033", "0086", "0123", "0153"]
informed-by:
  - 'owner, 2026-11-26: "option 2: launch covers all agents, remove spawn; drop unknown CLIs and non-agent processes; --force as a field for the override behaviour".'
  - 'RCA, 2026-11-26: spawned panes lost their chat log and stored no model or account.'
---

# 0163. Launch starts every agent, and spawn is removed

## Context and Problem Statement

`swarm spawn <id> <role> -- <cmd>` ran any command in a pane. `swarm launch` built the command
from a role. Since 2026-10-11, spawn made 176 child starts and launch made 4486, so spawn was
about 4%. Most spawn starts picked a model or a level on a non-chat role, or started pi with
`--no-skills`.

Spawned panes lost data that the app needs:

- 58.5% of spawned panes had no chat log, against 2.4% for launch.
- 76% stored no model, and 100% stored no account.
- The app then showed "No chat yet" and "Default model" for these panes.
- A spawned pi pane never got a log. Only launch gives pi `--session-id <session>-<agent>`
  (ADR 0086), and the app finds the chat by that id.

## Decision Outcome

`swarm spawn` is removed. `swarm launch` starts every agent.

- Launch takes `--model` and `--effort` on every role. A non-chat role without `--provider`
  uses the provider of the role's first runner. The run keeps the role's sandbox, approval and
  permission for that provider. There is no fallback runner (ADR 0033).
- A new `--force` flag, with no value, lets the flags that the role owns through in the extra
  args after `--`. Examples: pi `--no-skills` and `-e`/`--extension`; codex `--sandbox` and
  `--ask-for-approval`; claude and AGY `--permission-mode`.
- Launch refuses some flags even with `--force`:
  - Model and level flags: `--model`, `-m`, `--effort`, pi `--thinking` and pi `--provider`.
    Use launch `--model` and `--effort`.
  - Pi session flags: `--session-id`, `--session`, `--session-dir`, `--no-session`,
    `--continue`, `-c`, `--resume`, `-r` and `--fork`. They replace pi's fixed log id
    `<session>-<agent>`, which the app uses to find the chat.
- Unknown agent CLIs and non-agent processes in panes are no longer supported.
- `swarm spawn ...` prints
  `swarm: spawn was removed; use swarm launch <id> <role> [--provider p] [--model m] [--effort e] [--force] [-- args]`.

This supersedes the spawn parts of ADR 0008, 0123 and 0153. The `swarm spawn -- <cmd>` line of
0123 and the raw `swarm spawn` commands of 0153 no longer exist.

### Consequences

- Good, because every child pane stores its model and account and gets a chat log.
- Good, because a pi child always gets its fixed log id.
- Bad, because a command that no agent CLI provider covers can no longer get a swarm pane.
- Bad, because scripts that call `swarm spawn` fail until they move to `swarm launch`.
