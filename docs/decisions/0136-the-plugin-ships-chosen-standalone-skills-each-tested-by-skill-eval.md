---
status: accepted
date: 2026-11-05
deciders: [user]
related: ["0132", "0133"]
informed-by:
  - 'owner, 2026-11-05: "if a model can explain a particular thing or a particular verb without researching based on its weights then we should use that term. All the skill names will have to be done like that."'
  - 'owner, 2026-11-05: "We do not want each and every skill block to be part of our skill set ... we will go one by one creating these skills"; "Let us do it" on the list skill-eval, council, research, code-review, pair-programming'
  - 'cold name test 2026-11-05 (Claude, GPT, and Gemini panes in an empty folder): all three read skill-eval as "test a skill and score it"; all three answered "unsure" for hunter-challenger'
  - 'SkillsBench (arXiv 2602.12670): curated skills raised the pass rate from 33.9% to 50.5%; focused skills beat large bundles; self-generated skills did not replace curated ones'
  - 'anthropics/skills skill-creator (Apache-2.0) and obra/superpowers writing-skills (MIT): run each task with and without the skill before trusting it'
---

# 0136. The plugin ships chosen standalone skills, each tested by skill-eval

## Context and Problem Statement

ADR 0132 made `swarm` the one workflow skill and every verb a preset. The owner names a verb, and
the agent copies the matching preset, so 50 of 53 saved plans used a preset. The owner now wants a
small set of skills of their own, each named with a term that a model knows without research, and
each proven to help. Nothing in the kit tests whether a skill or block helps.

## Considered Options

- Ship chosen standalone skills beside `swarm`, one change each, and test each with a new
  `skill-eval` skill (chosen).
- Keep only presets, and test them by hand. No run shows whether a preset beats no preset.
- Install a public workflow pack, such as Superpowers. Its review, planning, and debugging skills
  overlap the kit's, and two near-equal descriptions make the model pick the wrong one.

## Decision Outcome

Chosen option: chosen standalone skills, because each one is then a known term with a test.

- `skill-eval` ships first. It checks a name with three cold models, runs real tasks with and
  without the skill in visible panes, grades the pairs blind, and keeps, fixes, or cuts the skill.
  Its grader prompts are adapted from Anthropic's `skill-creator` under Apache-2.0, with the
  licence in `skills/kit/skills/skill-eval/references/`.
- Each later skill is its own change. Before it is built, the same failure was seen in at least
  two real runs, its name passed the cold name test, and `skill-eval` shows that runs with it beat
  runs without it.
- File-format and tool skills from vendors, such as `pdf` or `xlsx`, are installed when a task
  needs them and never edited. A failing one is removed or replaced.
- `swarm` stays the front door for planned runs, and the presets stay until their skill lands.

### Consequences

- Good, because each skill name is one that a cold model explains correctly, and each kept skill
  has a recorded win in `evals/results.md`.
- Bad, because the CLI menus grow again, which ADR 0132 tried to stop. The two-run rule and the
  eval keep the set small.
- Bad, because an eval run costs cases × 2 × 2 panes, plus a grader per pair.
- Bad, because a baseline pane can still see installed skills in its catalog. Its brief tells it
  not to load the skill, and the grader voids a pair whose `notes.md` shows the skill was read.
- Open: whether a preset is removed in the same change that makes it a skill is decided with the
  first such change (`council`).

## Confirmation

`skills/kit/scripts/test_plugin_skills.py` lists the shipped skills, and each skill's description
passes its 40-word "Use when" check.

Supersedes ADR 0132 only where it says the plugin ships exactly three skills.
