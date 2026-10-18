---
name: plan
description: Plan any task from the kit's blocks, show the plan to the user as a flowchart in Plannotator, and run it as a step run after they approve. Use for "plan this", "plan and do", or a task that has no skill of its own.
---

# Plan

For the approval gate, apply `blocks/worker-brief/SKILL.md`. The default is to return the checked draft and leave its steps unstarted.


The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.

1. Write the plan by `blocks/planner/SKILL.md`.
2. Check it and draw it by `blocks/plan-chart/SKILL.md`.
3. Get the user's approval by `blocks/plan-review/SKILL.md`. Nothing runs before that.
4. Run the approved steps by the kit's `references/step-run.md`. The agent that takes a step reads and follows `blocks/<id>/SKILL.md` for each id on that step's `Blocks:` line in `plan.html`, in the shown order. The resolved list follows `blocks/blocks.toml`.
