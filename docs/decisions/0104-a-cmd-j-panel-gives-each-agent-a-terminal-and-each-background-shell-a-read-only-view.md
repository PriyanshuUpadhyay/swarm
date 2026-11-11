---
status: accepted
date: 2026-10-21
deciders: [user]
related: ["0099", "0029", "0083", "0023", "0003"]
informed-by:
  - 'owner, 2026-10-21: "need a way to look and manage background agents, shells and need a terminal access in each. toggled via cmd j"'
  - 'owner, 2026-10-21: confirmed at the review close ("take your recommendations, ADR yes")'
  - 'council 2026-10-21 (docs/council/2026-10-21-bg-panel.md in the owner dotfiles): GO-WITH-CHANGES, no blocker, 10 required changes'
  - 'probe 2026-10-21: portable-pty drops its writer with a newline and EOF, which ended a live pane shell through tmux attach'
---

# 0104. A Cmd+J panel gives each agent a terminal and each background shell a read-only view

## Context and Problem Statement

The owner could not see or reach the work that runs behind a chat. A child agent's pane was only a
`swarm attach <id>` command to copy, and the shells and subagents that a Claude agent starts in
the background did not show anywhere. ADR 0099 keeps the chat to what the agent CLI shows and
rejected a terminal inside the chat area, so the answer could not go into the chat.

## Considered Options

- A bottom panel, hidden until Cmd+J, that lists the open chat's live agents and their Claude
  background tasks, with an xterm.js terminal for the selected row.
- A terminal inside the chat area (rejected by ADR 0099).
- tmux control mode for the terminal transport.
- Keep the copied `swarm attach` command only.

## Decision Outcome

Chosen: the bottom panel, because it adds what the owner asked for and leaves the chat as ADR 0099
set it. An agent row runs the adapter's `attach` in a PTY on the server, over a WebSocket at
`/api/terminal` that takes a single-use ticket bound to its target; the socket also checks the
Host and the app Origin.
Only the tmux-swarm adapter has one session per agent, so only it gets a terminal; its `attach`
verb puts the window back at its first size when the last viewer leaves. A background shell row is
a read-only view of the output file that Claude Code names in its log, opened once after a check
that it is this user's task file under `/tmp/claude-<uid>/`. The page keeps one terminal live at
a time, the server allows 8, and closing one never stops the agent. tmux control mode was rejected because nothing else attaches to
these sessions.

### Consequences

- Good: each agent's real CLI is one keypress away, and a background shell's output is visible.
- Good: the chat itself does not change.
- Bad: the server now runs a PTY child per open terminal and has a socket route outside the
  bearer header, guarded by the ticket and the Host and Origin checks instead.
- Bad: a background task can be stopped only from its agent's own terminal (`/tasks`); Codex and
  AGY background work, a plain workspace shell, and other chats' tasks are out of v1.
- Bad: shell rows depend on the wording of Claude Code's launch result and task notification, so
  a change to that wording hides the rows until the reader learns it.

### Amendment 2026-11-11: a status line at the end of the chat

The chat ends with one status line, "Running · <label>", while background work runs. It shows a
count when several tasks run, a timer, and Open, which opens that task's row in this panel. It is a
line of text, not a terminal, so ADR 0099 holds; the output stays in the panel. It reads the same
`/api/background` list as the panel, so nothing is parsed twice.

- Codex background terminals are now listed. The reader takes a unified-exec session from a
  `Process running with session ID <n>` answer to `exec_command`, and ends it when a later
  `write_stdin` poll answers `Process exited with code <c>`. A terminal that exits with no later poll
  shows as running while its agent lives. This replaces "Codex background work is out of v1" above.
  The record shape comes from Codex's public behavior, not a repo fixture, so a change to that
  wording hides the rows.
- AGY background tasks are listed. A `GENERIC` step with status `RUNNING` and the line `Tool is
  running as a background task with task id: <conv>/task-<n>` starts a task; its label is the
  `Task Description:` line (`sleep 40`, or `Timer: 8s, Prompt: ...` for a `schedule` timer). A later
  `SYSTEM_MESSAGE` step with `sender=<that id>` ends it: `Command exited with code <c>` gives
  completed or failed, and a timer's message, which has no exit code, gives completed. The row has no
  log, because the task's `.log` file is under AGY's own folder and the panel's read-only view opens
  only a file under `/tmp/claude-<uid>/`. The record shape was read from real AGY logs on
  2026-10-23, so a change to that wording hides the rows.
- pi draws nothing: 212 local pi sessions use only `bash`, `read`, `edit`, and `write`, so its log
  holds no background record, and its core has no background command or agent.
- There is no Stop button. Claude stops a task only through `/tasks`, and Codex has only a stop-all
  API, so the contract has no per-task stop.
