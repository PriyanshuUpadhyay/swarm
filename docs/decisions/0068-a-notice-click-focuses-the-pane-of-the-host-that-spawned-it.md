---
status: accepted
date: 2026-10-12
deciders: [user]
related: ["0044", "0058"]
informed-by:
  - "Owner request on 2026-10-12: a notice click goes to the agent pane in the host that spawned it"
---

# 0068. A notice click focuses the pane of the host that spawned it

## Context and Problem Statement

The app notice selected a chat in Swarm. The owner needs the waiting agent's pane in Herdr
or tmux. The store already records the session's adapter and each agent's pane.

## Considered Options

- Keep selecting the chat in Swarm.
- Focus the pane in the recorded host, with the current chat selection as a fallback.

## Decision Outcome

The app click runs `swarm focus <session_id> <agent_id>`. The command reads the session's
adapter and the agent's pane from the store, because the app runs outside the agent's pane.
An input notice names the first waiting agent. A done notice names the chair or orchestrator.
Herdr and tmux supply the adapter's `focus` verb. A successful focus returns without activating Swarm.
If focus fails or no agent id is available, the app keeps its current chat selection path.

The detached tmux-solo server uses `attach` and has no focus verb. Superset has no verified
focus verb because its CLI on this Mac could not run. When the app is off, osascript keeps
no click action, as ADR 0058 says. ADR 0044 still decides when a notice fires.

### Consequences

- Good: the notice brings the owner to the pane that needs attention.
- Bad: the host must still have that pane; a closed pane or unsupported host falls back to Swarm.
