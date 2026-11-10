---
status: accepted
date: 2026-11-05
deciders: [user]
related: ["0132", "0136"]
informed-by:
  - 'owner, 2026-11-05: "Yes, remove the preset and create the skill."'
  - 'council eval 2026-11-05, four runs (skills/kit/skills/council/evals/results.md): the role cards won full-flow-always each time and lost graph-store narrowly each time; owner weak keep'
---

# 0137. A verb that becomes a skill leaves presets in the same change

## Context and Problem Statement

ADR 0136 left one point open: when a verb becomes a standalone skill, is its preset removed in the
same change? `council` is the first such verb. A copy in `presets/` and a copy in `skills/` would
drift apart, and two near-equal descriptions make the model pick the wrong one.

## Decision Outcome

Chosen option: move the verb in one change, because then one file owns it.

- `presets/council/` moves to `skills/council/`, and `PRESET.md` becomes `SKILL.md` with the same
  `## Preset` section, references, and evals.
- `preset.py` reads `skills/<verb>/SKILL.md` when `presets/<verb>/PRESET.md` is gone, so a plan line
  `Preset: council` and the step run work as before.
- The `swarm` front door still lists the verb, and points to the skill file.

### Consequences

- Good, because one file owns each verb, and the CLI menus show `council` as a skill.
- Bad, because the preset loader now reads two folders.

Resolves the open point of ADR 0136.
