---
status: accepted
date: 2026-10-29
deciders: [user]
amends: ["0099", "0120", "0123", "0126"]
related: ["0084", "0104", "0125"]
informed-by:
  - 'owner, 2026-10-29: "I would want the PTY support for each of them, Claude, Codex, Pi, and AGY. We don''t have to support any other adapter. We would start moving to Pty and remove the Tmux dependency, and we would only let users choose Tmux if they have it available in their system."'
  - 'owner, 2026-10-29: accepted the four recommendations of the plan ("your recommendation is sound")'
  - 'probe 2026-10-29, 80x24 tmux panes, one 300 to 500 word answer with no tools: the screen word count grew during the answer for Claude (68, 131, 205), Codex (70, 105, 142, 229), and pi (61, 178, 196); AGY showed one cut spinner line and drew the text at turn end'
  - 'Claude log of 2026-10-29: one record per finished content block, with no partial text'
---

# 0131. Each chat runs on a PTY host, and the chat streams the current turn

## Context and Problem Statement

The chat builds its rows from the agent's log. Each CLI writes a block to its log only when the
block ends, so a long answer shows nothing in the chat until it is done, while the CLI's own
screen shows the text as it arrives. The app starts every chair in a tmux pane, and only the
Claude, AGY, and pi seats run under the `swarm pty` shim there (ADR 0123). A Codex seat in tmux
has no swarm process that sees its bytes. The PTY host of ADR 0126 has a `vt100` parser for each
pane of any CLI, but it holds only the children of a chair that opts in, and it lives inside the
chair. The owner wants the current turn to stream in the chat for Claude, Codex, pi, and AGY, and
wants swarm to stop needing tmux.

## Considered Options

- A detached PTY host per chat holds the chair and its children, and the chat shows the host's
  screen of the current turn as one live row (chosen).
- `swarm serve` holds the PTYs. Each update restarts serve (ADR 0125), and the chats would die.
- Keep tmux, and poll `capture` for the live row. tmux stays a dependency, and Codex in tmux has
  no shim.
- Read each CLI's structured token stream (Codex app-server deltas, pi `message_update`). Claude
  has no such stream in interactive mode, so it would still need the screen.

## Decision Outcome

Chosen: the first option, because only a PTY that swarm owns sees the bytes of all four CLIs, and
a host apart from serve lives through a serve restart.

- `swarm launch orchestrator` starts a detached `swarm pty host` for the chat, so a chat that the
  app starts and one that the CLI starts get the same host. The chair is the host's first pane,
  and its children are hosted panes, as in ADR 0126. The host ends when the chair's pane ends,
  as a tmux pane ends with its shell, not with the CLI in it. This
  reverses the rejection of a PTY daemon in ADR 0123.
- Codex runs on the host as the other CLIs do.
- `swarm pty attach` sends a `vt100` snapshot and then the live bytes, so the Cmd+J terminal of
  ADR 0104 works for a hosted pane.
- A setting chooses the terminal host, PTY or tmux. PTY is the default, and tmux is offered only
  when `tmux` is on `PATH`. A chat that runs in tmux stays there until it ends.
- While an agent works, serve reads its pane with the chat adapter's `capture` verb every 250 ms,
  and the chat shows one live row under the log rows. The live row holds the last block of the
  turn above the input box, without its mark, the status rows, and the hints: a Claude `⏺` or
  Codex `•` block, pi's text after its last tool, and AGY's spinner row. A tool block, or the rest
  of the sent message before the first block, shows no row. The page hides the row once a log row
  of the same turn holds its text, and at turn end the row goes away. This is a named screen read
  under ADR 0120 and an exception to ADR 0099's "nothing more", because the live row is a preview
  and never a record.

### Consequences

- Good: a new chat needs no tmux, and the chat shows Claude, Codex, and pi text as it arrives.
- Good: a serve restart keeps the chats and their stream.
- Good: the read uses `capture`, so a chat that runs in tmux gets the live row too.
- Bad: AGY draws its answer only at turn end, so its live row shows only the one cut spinner line
  until then.
- Bad: pi marks no block, so its live row starts after its first tool. A model bridge that sends
  its answer whole shows nothing until turn end.
- Bad: the live row is the text as the CLI draws it, wrapped at the pane width, and a CLI update
  that changes its prompt echo or composer can break the cut until the reader learns it.
- Bad: a host is a process per chat that outlives serve, and a chat ends only when its chair
  exits or the owner closes it.
- Bad: workspace shells (Cmd+J) stay on tmux until a later decision moves them, so tmux is still
  needed for them.
