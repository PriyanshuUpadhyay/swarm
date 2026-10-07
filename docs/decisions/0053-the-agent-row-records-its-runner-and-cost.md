---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0008", "0021", "0032", "0052"]
informed-by:
  - "Owner verdict on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-10 (a), cost from the Stop hook, G-6, G-7, G-26"
  - "src/store.rs AgentRow has no model, effort or account; SwarmApp keeps the launch model in memory; src/host.rs:147 maps the Claude and Codex Stop hook to done"
---

# 0053. The agent row records its runner, and the Stop hook keeps a cost total

## Context and Problem Statement

`swarm launch` resolves a profile, runner, model, effort and account, but the bus keeps none of
them. The app keeps the launch model in memory, so a restart loses it, and it reads only the
selected chat's transcript, so other chats show no model and no cost. A fallback runner start is
never shown.

## Considered Options

- `swarm launch` writes the runner data to the agent row in the bus database.
- The app parses launch output and keeps it in memory.
- The app reads each chat's transcript.

## Decision Outcome

Chosen: the agent row. `swarm launch` writes profile, runner label, model, effort and account. The
turn-end Stop hook updates the model after a mid-chat switch and adds the turn's token usage and
cost to a total on the row. Usage for a chat sums the rows of every model session in its chain.
Rejected: app memory, because a restart loses it; reading every transcript, because it is slow
with many chats.

### Consequences

- Good: every row and tab can show model and cost cheaply, and the data survives a restart.
- Bad: the bus schema grows, and a provider whose hook does not report usage shows no cost.
