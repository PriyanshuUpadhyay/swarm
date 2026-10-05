---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0021", "0034"]
informed-by:
  - "Owner answers Q2 (b) and Q3 (1) on 2026-10-05, flow tmp/flow/2026-10-05-swarm-notify (03-contracts.md)"
  - "Council 2026-09-24, owner answer \"Chair only\" (docs/council/2026-09-24-roadmap-notes.md:81) and the notify plan (:104-106)"
  - "src/bus.rs:881-885 and ADR 0034: a chair started by hand runs no swarm state hook"
---

# 0044. Swarm notifies on a change to waiting, and the chair notifies when done

## Context and Problem Statement

The owner works away from the screen while a swarm runs. The owner wants one notice when an agent
waits on a permission or a question, and one when the run is done, and no repeats. Swarm learns an
agent's state from provider hooks and a screen check (ADR 0021). A chair started by hand has no
swarm state hook (ADR 0034), so swarm cannot see when such a chair is done. The owner said to make
notices "part of the agents".

## Considered Options

- Swarm notifies on a change to `waiting`; the chair runs `swarm notify` at the end of its run; only
  the chair may call `swarm notify`.
- Hand chairs get state hooks too, through a global Claude hook and new shared hook text that finds
  the chair by its pane, so swarm also notifies on the chair's change to `done`.
- The sweep reads the chair pane each tick and notifies when it goes idle.
- Any agent may call `swarm notify`.

## Decision Outcome

Chosen: the first option. Swarm sends one notice when an agent's stored state changes to `waiting`,
from a hook or from the screen check, and sends none when the state was already `waiting`. The
chair runs `swarm notify <title> [--body <text>]` when its run is done, as the orchestrator skill
and the chair host contexts tell it. `swarm notify` refuses a worker (`swarm::host::is_worker`), so
a worker reports to the chair. Rejected: hand-chair hooks, because they need two writes to owner
config and a Codex re-trust of every hook hash for one notice; the sweep poll, because it works
only on Herdr, is up to 30 s late, and needs a running sweep; any agent calling, because N children
flood the owner and repeat the waiting notice.

### Consequences

- Good: one notice per change to `waiting`, from every agent, the app chair included.
- Good: `swarm notify` needs no swarm session, so a hand chair and the owner's shell can call it.
- Bad: the done notice depends on the chair model following its skill line.
- Bad: a hand chair that waits on a permission sends no notice, because it has no state hook.
- Neutral: stall notices come from swarm itself (flow ring-proof), because a stalled chair cannot
  run `swarm notify`.
