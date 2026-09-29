---
status: accepted
date: 2026-09-29
deciders: [user]
related: ["0003", "0005", "0008", "0015"]
informed-by:
  - "User answers on 2026-09-29 (states from the hook system; screen check now; trust the Codex hook once)"
  - "https://code.claude.com/docs/en/hooks"
  - "https://learn.chatgpt.com/docs/hooks"
  - "https://antigravity.google/docs/hooks/"
  - "Herdr 0.9.1 docs, agents.mdx and integrations.mdx"
---

# 0021. Agent state comes from provider hooks and a pane screen check

## Context and Problem Statement

The app must show each agent as Working, Waiting for you, Done, Failed, or Ended. Swarm knows
only whether the agent process is alive. Claude Code, Codex, and Antigravity CLI (AGY) each have
hooks, but no provider covers every state: AGY has no approval event, Codex has no failure event,
and Claude and Codex send nothing after Esc or a denied prompt.

## Decision Drivers

- The owner wants the states to come from the hook system.
- A hook must never block or fail an agent's turn.
- The user's own hooks and settings must keep working.

## Considered Options

- Provider hooks only.
- Provider hooks, plus a check of the pane's bottom rows for the gaps.
- Screen check only, as Herdr does for state.

## Decision Outcome

Chosen: hooks plus a screen check. `swarm hook <provider>` maps each hook event to a state and
writes it to new `agent.state` columns. `swarm launch` injects the hooks (Claude `--settings`,
Codex `-c` with one fixed command the user trusts once, and one `swarm` group in AGY's global
`hooks.json`). The agent listing reads the bottom rows of live panes (`tmux capture-pane`, or
`herdr agent get` for Herdr panes); a hook report wins for 10 s, then a differing screen result
wins.

### Consequences

- Good: states change the moment a hook fires, and the screen check closes every gap.
- Good: the UI reads one field from swarm; no provider detail reaches the app.
- Bad: screen patterns break when a provider changes its TUI, so they need fixture tests and
  upkeep.
- Bad: Codex shows no hook state until the user trusts the hook in `/hooks`.
- Bad: swarm writes a group into the user's global AGY `hooks.json`.

## Pros and Cons of the Options

### Hooks only
- Good, because it is exact and cheap.
- Bad, because AGY Waiting, Codex Failed, and the state after Esc stay wrong.

### Hooks plus screen check
- Good, because it is exact where hooks exist and complete elsewhere.
- Bad, because it adds screen patterns to maintain.

### Screen check only
- Good, because it needs no config change in any provider.
- Bad, because it is slower and misses states that do not show on the last rows.
