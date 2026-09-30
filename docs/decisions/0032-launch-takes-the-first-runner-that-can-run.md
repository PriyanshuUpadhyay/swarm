---
status: accepted
date: 2026-09-30
deciders: [user]
related: ["0004", "0008", "0031"]
informed-by:
  - "User answer on 2026-09-30 (flow 01-frame): fallback triggers at launch only (missing CLI, no signed-in account, low usage); no mid-run switch"
  - "yelo profile list --usage --json took 0.08 s on the owner's Mac on 2026-09-30"
---

# 0032. Launch takes the first runner that can run

## Context and Problem Statement

A profile lists runners of any provider. Before this, a later runner took a seat only when the
first runner's CLI was not on PATH (`substitutes`). When every account of a provider was out of
usage, the launch still started that provider. ADR 0004 had rejected a usage fallback.

## Considered Options

- Keep substitutes for a missing CLI only.
- At launch, skip a runner whose CLI is missing, whose accounts are all signed out, or whose best
  account has less usage left than a threshold.
- Also switch a live agent to the next runner when it hits a quota mid-run.

## Decision Outcome

Chosen: skip at launch. The threshold is `min_usage_left_pct` in `profiles.json`, default 5. The
account read has a 2 s deadline, and a read that fails, times out, or finds no accounts counts as
"can run", so a Mac without yelo still launches. Launch prints one line per skipped runner, and
fails with one line per runner when none can run.

### Consequences

- Good: a role keeps working when one provider runs out, with no config edit.
- Bad: a live agent that hits its quota still needs a manual handoff (ADR 0017), and a stale usage
  number can pick a runner that then fails in its pane.
