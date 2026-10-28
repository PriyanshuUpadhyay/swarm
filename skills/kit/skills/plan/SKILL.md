---
name: plan
description: Plan from a verb's preset or the block catalog, show the chart, get approval, run the steps. Use when a verb skill sends you here, for "plan this", "plan and do", or a task with no skill of its own.
---

# Plan

Example. The owner types `/research which state library`. The `research` skill sends the task
here with its preset. The planner copies the preset into `plan.md`, adds no block, and the chart
shows six steps. The preset says `Stop: owner`, so the owner clicks approve in Plannotator, and
`start_plan.py` makes the six step files.

For the approval gate, apply `blocks/worker-brief/SKILL.md`. The default is to return the checked draft and leave its steps unstarted.

The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.

## Presets

A preset is a verb's start list of steps and blocks. It is a `## Preset` section in the verb's
`SKILL.md`, or in `references/<mode>.md` for a verb with modes. The section has one line
`Stop: owner` or `Stop: chat`, an optional line `Start: <command>` for a verb that runs its own
step run, an optional line `Offers: <block>, <block>` with the blocks that the planner may add for this
verb (no step loads them; they are off by default), and one table in the plan step-table shape that `blocks/planner/SKILL.md` sets, with no
Writes column. The planner adds Writes, so the Writes and duty rules apply to the plan, not the preset.
`python3 -I <kit root>/skills/plan/scripts/preset.py <verb> [<mode>]` checks a preset and prints
each step with the blocks it resolves to. A task with no verb has no preset.

## Steps

1. Write the plan by `blocks/planner/SKILL.md`.
2. Check it and draw it by `blocks/plan-chart/SKILL.md`.
3. With `Stop: owner`, or with no preset, get the owner's approval by `blocks/plan-review/SKILL.md`.
   Nothing runs before that. With `Stop: chat`, show the chart and the blocks table of `plan.md` in
   the reply, and go on.
4. Start the steps with `python3 -I <kit root>/blocks/plan-review/scripts/start_plan.py <run>`. The
   script owns the stop rule. For a preset with a `Start:` line, it makes no step files and prints
   that command after the check passes, so run that command next.
5. Run the steps by the kit's `references/step-run.md`. The agent that takes a step reads and follows `blocks/<id>/SKILL.md` for each id on that step's `Blocks:` line in `plan.html`, in the shown order. The resolved list follows `blocks/blocks.toml`.
