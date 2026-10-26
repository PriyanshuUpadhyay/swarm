---
status: accepted
date: 2026-10-26
deciders: [user]
amends: ["0123"]
related: ["0120", "0084", "0125"]
informed-by:
  - 'owner, 2026-10-26: children may die with their orchestrator, so no long-lived daemon is needed'
  - 'owner, 2026-10-26: "Start the phase directly. We don''t need ZMX." and "Auto." after approving the plan in Plannotator'
  - "parser trial 2026-10-26 at 100x30 against tmux 3.8: vt100 and @xterm/headless matched all 5 final screens; alacritty_terminal missed 15 of 20 cut screens on synchronized output"
  - "report ~/.claude/reports/2026-10-26-pty-host-for-agent-clis.md: zmx and shpool answer no OSC 7501 probe and feed no status-feed; shpool takes one client at a time"
---

# 0126. The chair's swarm pty hosts its children on PTYs it owns

## Context and Problem Statement

ADR 0123 put each Claude, AGY, and pi seat on a PTY that `swarm pty` owns, inside its own tmux
pane, and it rejected a PTY daemon that replaces tmux, because that daemon would have to own
attach, capture, and pane lifetime. The owner now accepts that children die with the chair that
launched them. So a process that lives as long as the chair can own the children's PTYs, and
the chair's own `swarm pty` is that process.

## Considered Options

- The chair's `swarm pty --host` owns its children's PTYs (chosen).
- `swarm serve` owns them. It restarts on each update (ADR 0125), and the children would die.
- zmx or shpool as the host. Neither answers the OSC 7501 probe or feeds `status-feed`.
- Keep a tmux session per child.

## Decision Outcome

Chosen: the first option, because it needs no new long-lived process, and the shim already
answers the probe.

- A chair that `swarm launch orchestrator` starts with `SWARM_CHILD_HOST=pty` runs under
  `swarm pty --host --`, for any provider. Without the variable, launch works as before, and
  tmux stays the default.
- The host listens on `/tmp/swarm-<uid>/<hash>.sock`, mode 0600 in a 0700 folder, where the hash
  comes from `runs/<session>`. A macOS socket path holds at most 104 bytes, and a branch build's
  `runs/<session>/pty.sock` is longer. The chair gets `SWARM_PTY_HOST`. `list` asks each socket
  in the folder. Hosts start one at a time under a lock on the folder, and a starting host
  removes each socket there that refuses connections, which a killed host leaves.
- A host that cannot start prints one line, and the chair runs without it, so its children open
  tmux panes. The host is opt-in, and a chair that runs is worth more than the missing host.
- A host that finds a live host at its socket does not start, so a second chair of the session
  runs with tmux panes and the first chair's children stay reachable. A chair with no host of its
  own drops an inherited `SWARM_PTY_HOST`.
- Each pane has one thread that writes its input and the query replies in order, so the bytes of
  one write do not mix with another's. A ring is two writes, its text and then Enter, so two rings
  sent at the same time can still mix, as in tmux. The queue holds 64 writes of any size, where
  tmux keeps all of them. When a child reads no input and the queue is full, a write fails, and
  a query reply is dropped, so the host goes on reading the child's output. A reply waits behind
  the writes queued before it, so a CLI that asks while a large ring is queued gets it late.
- A host waits at most 5 s for the folder lock, and then the chair runs with no host.
- A pane whose reader thread panics leaves the list, and its shell is hung up.
- A host numbers its panes from its start time in ms, so a pane id that an earlier chair of the
  session stored never names a child of a later one.
- The session adapter stays `tmux-swarm`, because the app lists only tmux-swarm chats (ADR
  0084). Its spawn sends a chair with `SWARM_PTY_HOST` to the host, and each verb sends a
  `pty:<session>:<n>` pane to `swarm pty <verb>`. Other panes go to tmux as before.
- A child is the owner's login shell at 80x24, as a detached tmux session starts it, and launch
  rings the line into it. Its bytes go to a `vt100` parser with 2000 rows of history, and to the
  `$SWARM_FEED` line, which drops bytes when that pipe is full.
- No terminal stands behind the host, so it answers the OSC 7501 probe, DA1, DSR 5n and 6n, and
  OSC 10 and 11 in the order they arrive. A CLI that gets no cursor report can fail to start.
- When the chair exits, the host sends SIGHUP to each child's shell and foreground job. When the
  host is killed, the kernel hangs the children up as their masters close.

### Consequences

- Good: a seat needs no tmux session, and `send`, `answer`, `interrupt`, `key`, `screen`,
  `capture`, and `close` reach it. With no tmux installed, `list` names the hosted panes alone.
- Bad: a hosted pane has no attach yet, so the app cannot show it as a column. Phase 2 sends a
  `vt100` snapshot and then the live bytes to the app.
- Bad: a process that inherits `SWARM_PTY_PANE` and starts the `-L swarm` tmux server passes
  it to that server's panes, and `self` there names the hosted pane. Hosted workers are leaves
  and start no chair, so no current path does this.
- Bad: a chair restart ends its children, and a seat that the app launches outside the chair
  still opens a tmux pane.
- Later: make the host the default after a real session with Claude, Codex, AGY, and pi seats.
