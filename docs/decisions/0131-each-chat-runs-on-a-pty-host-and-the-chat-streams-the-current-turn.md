---
status: accepted
date: 2026-10-29
deciders: [user]
amends: ["0099", "0120", "0123", "0126"]
related: ["0084", "0104", "0118"]
informed-by:
  - 'owner, 2026-10-29: "I would want the PTY support for each of them, Claude, Codex, Pi, and AGY. We don''t have to support any other adapter. We would start moving to Pty and remove the Tmux dependency, and we would only let users choose Tmux if they have it available in their system."'
  - 'owner, 2026-10-29: accepted the four recommendations of the plan ("your recommendation is sound")'
  - 'probe 2026-10-29, 80x24 tmux panes, one 300 to 500 word answer with no tools: the screen word count grew during the answer for Claude (68, 131, 205), Codex (70, 105, 142, 229), and pi (61, 178, 196); AGY showed one cut spinner line and drew the text at turn end'
  - 'Claude log of 2026-10-29: one record per finished content block, with no partial text'
  - 'owner, 2026-10-30, on a pi chat whose live row showed a running bash box, its Elapsed clock, the queued steering message, and landed text again: "The bash commands do not have to stream and then the steering message does not have to stream." and "We just care about streaming the thinking part as well as the final output … all the tool calls … should not show up as stream in the chat"'
  - 'council 2026-10-30 (~/dotfiles/docs/council/2026-10-30-pi-live-row-stream.md), unanimous: only bash and codemode draw a pi Took row, so no screen cut can find the end of a read or edit box; pi gives message_update to extensions in interactive mode'
  - 'probe 2026-10-30, pi 1.2.0 with claude-bridge Haiku 5.5 in a hosted pane: the extension file held thinking that grew 188 to 1586 characters and then text that grew 1555 to 3226 in 250 ms steps; it held no tool, clock, or queued text, and it was empty between messages'
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
- `swarm serve` holds the PTYs. Each update restarts serve (ADR 0118), and the chats would die.
- Keep tmux, and poll `capture` for the live row. tmux stays a dependency, and Codex in tmux has
  no shim.
- Read each CLI's structured token stream (Codex app-server deltas, pi `message_update`). Claude
  has no such stream in interactive mode, so it would still need the screen. pi takes this
  option for its own live row (see below), because its screen marks no block.

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
  Codex `•` block under the latest echo of a sent message, and AGY's spinner row. A tool block,
  the rest of the sent message before the first block, or a block above a message queued while
  the turn runs shows no row, and so does a screen that still shows the ended turn's end row
  (Claude's `✻ Sautéed for 1s · done …`, Codex's `Worked for 37s • …`).
- pi's live row does not read the screen. `swarm launch` loads swarm's extension into each pi
  pane (`--extension`, `src/bus/pi_live.ts`), which writes the `thinking` and `text` blocks of the
  assistant message that still streams to `runs/<session>/<agent>.live` at most each 250 ms, and
  empties it when the message ends. Serve reads that file in place of `capture`. So a running
  tool, its clock, a queued message, and text that the log already holds never show. The page
  shows the thinking as an open Thinking row above the text. Tool calls show only as the log's
  folded cards. A hosted pane that no one views is
  80 columns by 100 rows, so a turn's whole answer stays on the screen; a tmux pane keeps its
  size. The page types new text past what already shows out over a few frames, so the row grows
  smoothly and not a screen line at a time. A change inside the shown text, such as a digit,
  shows at once and does not type the rest out again.
- Claude and Codex hide their working row while they write the answer. Text under the latest
  echo of a sent message with no end row reads as working, so the screen read of ADR 0070 does
  not end the turn early. The page hides the row once a recent log row
  holds its text, a row of an earlier turn included. A turn or a pi message can end seconds
  before its log rows land, so at turn end a text that a
  log row will hide stays until that row lands. A short spinner text goes at once. A new chair's hook stores its log only at the end of its first
  turn, so serve finds a live chat's log by the chair's id until then, and the first turn shows
  live too. This is a named screen read
  under ADR 0120 and an exception to ADR 0099's "nothing more", because the live row is a preview
  and never a record.

### Consequences

- Good: a new chat needs no tmux, and the chat shows Claude, Codex, and pi text as it arrives.
- Good: a serve restart keeps the chats and their stream.
- Good: the read uses `capture`, so a chat that runs in tmux gets the live row too.
- Bad: AGY draws its answer only at turn end, so its live row shows only the one cut spinner line
  until then.
- Good: pi's live row comes from its own events, so it shows its thinking and text and never a
  tool box, a clock, or a queued message.
- Bad: a pi pane that `swarm launch` did not start has no extension, so it shows no live row. A
  pi message that a model writes in under 250 ms shows nothing until its log row lands.
- Bad: pi's extension API can change in a pi update; the event names are documented, so this is
  less fragile than its screen, but a change stops pi's live row until swarm follows it.
- Bad: a Claude or Codex version that drops its end row reads as working once the hook report is
  stale, until the next turn.
- Bad: the live row is the text as the CLI draws it, wrapped at the pane width, and a CLI update
  that changes its prompt echo or composer can break the cut until the reader learns it.
- Bad: a host is a process per chat that outlives serve, and a chat ends only when its chair
  exits or the owner closes it.
- Bad: the background shells of a workspace (`src/serve/shells.rs`) stay on tmux until a later
  decision moves them, so tmux is still needed for them. Cmd+J shows a hosted agent's pane through
  `swarm pty attach`, and those shells through tmux.

## Decision Drivers

- The owner wants the current turn to stream in the chat for Claude, Codex, pi, and AGY.
- The owner wants swarm to stop needing tmux, and to offer tmux only when it is installed.
- A chat must live through a serve restart, because each update restarts serve (ADR 0118).
- The log stays the record of a turn (ADR 0099), so a preview must never land as a row.

## Pros and Cons of the Options

### A detached PTY host per chat, and a live row from its screen

- Good, because one reader serves all four CLIs, and a new chat needs no tmux.
- Good, because the host lives apart from serve, so a serve restart keeps the chat.
- Bad, because the cut of the screen depends on each CLI's layout, which can change.
- Bad, because each chat adds a process that outlives serve.

### `swarm serve` holds the PTYs

- Good, because no new process is needed.
- Bad, because a serve restart ends every chat.

### Keep tmux, and poll `capture`

- Good, because the code and the panes stay as they are.
- Bad, because tmux stays a dependency, which the owner wants to remove.

### Read each CLI's structured token stream

- Good, because the text arrives as the CLI sends it, with no screen cut.
- Bad, because Claude has no such stream in interactive mode, so it still needs the screen.

## Confirmation

- `cargo test --test launch an_app_chair` checks that an app chair runs on its own host when tmux
  is not on `PATH`, and that its children end with it.
- `cargo test --test pty_host` checks attach, the viewer's end with its pane, and the cut of the
  host's query answers.
- `src/screen/live_tests.rs` checks the turn cut on real screens of Claude, Codex, and AGY,
  with a message queued while the turn runs, and that a pi screen gives no text. A CLI update
  that breaks the cut fails these tests once its new screen is captured.
- `serve::events::tests` checks the `live` frame of a chair chat and its null at turn end, and
  that a pi chair sends its extension file's `{thinking, text}` and not its screen.
- `app-stream.test.ts` checks that the text stays after turn end until the log's rows land, that
  only new text types out, and that pi's thinking shows as an open row, and
  `cargo test --features serve --test serve a_claude_chair_log` checks the log found by the id.

## Informed by

- The screen probes and the Claude log of 2026-10-29 in the front matter.
- `src/cli/pty_host.rs` and ADR 0126, the host that this record extends.
- `src/serve/follow.rs` and ADR 0099, the log-only chat that this record amends.

## Links

- Amends: 0099, 0120, 0123, 0126
- Related: 0084, 0104, 0118
