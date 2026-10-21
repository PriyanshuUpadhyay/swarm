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

# 0102. A Cmd+J panel gives each agent a terminal and each background shell a read-only view

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
`/api/terminal` that takes a single-use ticket bound to its target, the Host, and the app Origin.
Only the tmux-swarm adapter has one session per agent, so only it gets a terminal; its `attach`
verb puts the window back at its first size when the last viewer leaves. A background shell row is
a read-only view of the output file that Claude Code names in its log, opened once after a check
that it is this user's task file under `/tmp/claude-<uid>/`. One terminal is live at a time, and
closing it never stops the agent. tmux control mode was rejected because nothing else attaches to
these sessions.

### Consequences

- Good: each agent's real CLI is one keypress away, and a background shell's output is visible.
- Good: the chat itself does not change.
- Bad: the server now runs a PTY child per open terminal and has a socket route outside the
  bearer header, guarded by the ticket, Host, and Origin instead.
- Bad: a background task can be stopped only from its agent's own terminal (`/tasks`); Codex and
  AGY background work, a plain workspace shell, and other chats' tasks are out of v1.
- Bad: shell rows depend on the wording of Claude Code's launch result and task notification, so
  a change to that wording hides the rows until the reader learns it.
