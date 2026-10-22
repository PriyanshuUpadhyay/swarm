---
status: accepted
date: 2026-10-22
deciders: [user]
amends: ["0111"]
informed-by:
  - "pi chat 01a14571, 2026-10-16: show per skill the blocks it turns on, the duty blocks, and the blocks the planner may add"
  - "~/dotfiles/docs/council/2026-10-21-codeaf-rules-not-workflow.md (change 4)"
  - 'owner, 2026-10-22: "yes both /skill:implement auto"'
---

# 0113. A preset may offer blocks, and the Skills page shows the plan front door and duty blocks

## Context and Problem Statement

ADR 0111 shows each preset's steps as a graph. The owner also wants to see, per skill, which
blocks the planner runs before step 1, which blocks a plan must keep, and which blocks the
planner may add. The kit had no field for the last one, so the page could only guess it from
block metadata.

## Considered Options

- An optional `Offers: a, b` line in a preset, owned by the preset.
- Offers derived from each block's `users` metadata.

## Decision Outcome

Chosen: an optional `Offers:` line, because one file then owns the list and `preset.py` can check
it. `preset.py` refuses an offered block that is a base block, a duty block (`plan_rules.is_duty`),
or listed twice. `preset.py --json` adds `front` (planner, plan-chart, plan-review), `offers`,
`duty`, and `offered_by`; each field is absent when empty. A plan may list an offered, unused
block as `Offered` in its ledger; `plan_chart.py` accepts it, and `step_amend.py` turns it into
`Chosen` when a step adds it. The Skills page shows the front door strip, marks duty blocks on
their steps, and lists offered blocks in an "Offered" tray as off.

### Consequences

- Good: the page, the planner, and the plan check read one list.
- Good: an old kit without these fields still loads; the page treats each field as empty.
- Bad: a new block that a preset should offer needs an edit to that preset's `Offers:` line.

## Links

- Amends ADR-0111 (the Skills page and `preset.py --json`).
