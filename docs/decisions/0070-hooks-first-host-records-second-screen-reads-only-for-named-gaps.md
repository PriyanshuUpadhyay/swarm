---
status: accepted
date: 2026-10-12
deciders: [user]
related: ["0021", "0044"]
informed-by:
  - "~/dotfiles/docs/council/2026-10-12-osc7501-state-model.md"
  - 'owner, 2026-10-12: "screen read should be the last option in any setup of ours"'
  - "~/.claude/reports/2026-10-12-osc7501-program-status.md"
---

# 0070. Hooks first, host records second, screen reads only for named gaps

## Context and Problem Statement

ADR 0021 trusted a provider hook for 10 s, but any differing screen read could then overturn it.
Terminal prompt patterns drift across CLI releases and create false state changes.
Also, swarm did not record whether a waiting agent needed a permission or asked a question.

## Decision Drivers

- The owner rule: "screen read should be the last option in any setup of ours" (2026-10-12).
- Hooks provide exact state events, but some providers omit certain states.
- The owner needs to distinguish permission prompts from questions without opening the pane.

## Considered Options

- Keep the 10 s override: a hook wins for 10 s, then any screen read wins.
- Screen last with a gap table: hooks first, host records second, and screen reads only for named gaps.
- Hooks only: drop screen reads completely and accept missing states.

## Decision Outcome

We chose hooks first, host records second, and screen reads only for named gaps.
A fresh hook report stands until `STALE_S` (45 s) unless the screen state is in `screen::screen_gap`.
The gap table in `src/screen.rs` is the code owner of this rule:
- Codex Failed
- AGY Waiting
- Any provider Idle (after Esc or a denied prompt)

The `agent.state_kind` column carries `permission` or `question` during a wait.
A program-status record from a host slots in as a host record later.

## Consequences

- Hook reports stay authoritative and do not flutter when screen patterns fail to match.
- Owner notices and the UI show whether an agent waits on a permission or a question.
- Screen reads remain restricted to verified provider gaps and unhooked panes.
