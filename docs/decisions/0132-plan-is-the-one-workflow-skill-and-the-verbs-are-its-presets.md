---
status: accepted
date: 2026-10-31
deciders: [user]
related: ["0080", "0111", "0113"]
informed-by:
  - 'council 2026-10-30 (~/dotfiles/docs/council/2026-10-30-skill-consolidation.md), GO-WITH-CHANGES: take the useful parts of the third-party packs into kit blocks before any pack is removed'
  - 'owner, 2026-10-30, on the skill-tree page: there are no separate skills; blocks combine at run time from the task'
  - 'owner, 2026-10-31: "go" on the restructure table (plan the one skill; build, implement, review, research, explain, and council as presets; web-search and engineering-standards as blocks; orchestrate-* as run-workers rows; decisions removed)'
---

# 0132. Plan is the one workflow skill, and the verbs are its presets

## Context and Problem Statement

The kit shipped 15 skills. Six verb skills (build, implement, review, research, explain, council)
each only held a preset and sent the task to `plan`. Four more (web-search, engineering-standards,
and the three orchestrate adapters) were read by other skills, never typed by the owner. Each CLI
listed all 15 in its skill menu, next to about 70 third-party skills, so the menu was long and the
model often picked a pack skill over a kit block. The owner wants one entry point that builds the
run from blocks.

## Considered Options

- `plan` is the only workflow skill; each verb is a `presets/<verb>/PRESET.md` that `plan` reads
  (chosen).
- Keep the verb skills as thin pointers to `plan`. The menu stays long, and two files say when a
  verb applies.
- Fold every preset into `plan/SKILL.md`. The file grows past 1,000 lines, and the preset scripts
  lose their folders.

## Decision Outcome

Chosen option: `plan` is the only workflow skill, because one menu entry with a preset table picks
the verb, and each verb keeps its folder, scripts, and mode files.

- The plugin ships three skills: `plan`, `swarm-orchestrator`, and `swarm-voice`.
- `skills/kit/presets/<verb>/PRESET.md` holds each verb's rules and its `## Preset` section.
  `preset.py` and the Skills page read presets from there.
- `web-search` and `engineering-standards` are blocks. The standards rows are in
  `blocks/engineering-standards/references/standards.md`.
- `blocks/run-workers/SKILL.md` holds one runtime line each for Claude, Codex, and AGY, and the
  swarm host reference. The `[agent-runtime]` context points there.
- The `decisions` skill is removed. This index is kept by hand.

### Consequences

- Good, because a CLI menu shows one kit workflow entry, and the preset table in `plan` says which
  verb fits.
- Bad, because `/research` and `/review` stop being commands; the owner types `/plan research …`.
  `skills/kit/README.md` lists what to type now.
- Bad, because `~/AGENTS.md` and other dotfiles point at the old paths until the new kit is
  installed. They change in the same step as the install.

## Confirmation

`scripts/kit-checks.sh` checks that `skills/` holds exactly the three skills, that `presets/`
holds the six presets, and that every preset passes `preset.py`.
