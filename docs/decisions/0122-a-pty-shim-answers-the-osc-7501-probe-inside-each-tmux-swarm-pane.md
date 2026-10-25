---
status: accepted
date: 2026-10-25
deciders: [user]
related: ["0086", "0120", "0070", "0012"]
informed-by:
  - "Trial 2026-10-25: Claude 2.1.310 and AGY 1.4.5 in tmux 3.8 send no OSC 7501 report; under a PTY shim they send idle, working, and done"
  - "tmux issue #5713, closed 2026-10-13: the maintainer will not add OSC 7501 for now"
  - "Owner approved the plan in Plannotator and said: Approve and auto (2026-10-25)"
---

# 0122. A PTY shim answers the OSC 7501 probe inside each tmux-swarm pane

## Context and Problem Statement

Claude Code, AGY, and pi send OSC 7501 state reports only to a terminal that answers the
`OSC 7501 ; ?` probe. tmux does not answer it, and it answers the device-attributes query that
each CLI sends right after the probe, so the CLI decides that no reader is there. ADR 0120 asks
for a raw terminal that swarm owns. ADR 0086 worked around the gap for pi alone with
`PI_PROGRAM_STATUS=1`; Claude and AGY have no such variable.

## Considered Options

- A shim, `swarm pty -- <cmd>`, that runs the CLI on its own PTY inside the tmux pane.
- Answer the probe from the pane feed with `tmux send-keys`.
- A swarm PTY daemon that replaces tmux as the host.

## Decision Outcome

We chose the shim, because it gives every reporting CLI a terminal that answers the probe and
keeps tmux for attach, capture, keys, and pane lifetime.

- `swarm pty` copies bytes unchanged in both directions, passes the window size on SIGWINCH,
  and exits with the CLI's code. It writes the probe reply to the CLI before it passes that
  output on to tmux, so the reply arrives before tmux's device-attributes reply.
- `swarm launch` wraps Claude, AGY, and pi when the adapter's spawn line names `$SWARM_FEED`.
  A `swarm spawn -- <cmd>` line is not wrapped, because it may call a pane shell function.
  Codex sends no OSC 7501 and is not wrapped. Other adapters keep their own terminal, which may
  answer the probe itself (Rex).
- pi still starts with `PI_PROGRAM_STATUS=1`, so it reports on an adapter that does not wrap it.
- `swarm status-feed` stores the CLI's own reports beside the Claude or AGY hook states. Both
  say the same states, and the CLI's own report adds the cancel (`idle` after work) that no
  Claude or AGY hook gives. The feed looks for a pi log only in a pi pane.

Rejected options and why:
- `send-keys` answer: tested, Claude sent no report, because tmux's device-attributes reply
  came first.
- PTY daemon: a new host for attach, capture, and pane lifetime, for the same state signal.

### Consequences

- Each wrapped pane has one more process, `swarm pty`, between the pane shell and the CLI.
  The CLI does not inherit the PTY master, so it is hung up when `swarm pty` dies.
- A Claude or AGY pane now has two state writers, its hooks and its own reports.
- If `swarm pty` is killed and the pane stays open, the pane's terminal stays in raw mode. The
  pane shell then runs `swarm exited`, and the pane closes.
- An Esc cancel in a Claude or AGY pane is stored as `done` in under a second (0.6 s live),
  where a screen read took 7.8 s before.
- The shim uses `libc`, which is now a dependency of the default build.
