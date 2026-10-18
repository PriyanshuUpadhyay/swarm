---
name: plan-flow
description: "Keep the kit-root rule that the plan and comment-round blocks share with each selected part. Use when a step writes, checks, or approves a plan, or runs owner comment rounds on a page."
license: MIT
metadata:
  kind: parent
  owns: "shared plan and comment-round rules"
  serves: "plan, view and talk"
  needs: "a plan or a built page"
  gives: "shared rules for the selected plan parts"
  conflicts: ""
  users: "planner"
  source: "blocks/planner/SKILL.md, blocks/plan-chart/SKILL.md, blocks/plan-review/SKILL.md, blocks/comment-round/SKILL.md, and blocks/page-comments/SKILL.md (each: the kit root is two folders up from this file)"
  parts: "planner, plan-chart, plan-review, comment-round, page-comments"
---

## Rules
- The kit root holds `blocks/` and `references/`. It is two folders up from this file and from each part's file.

Each part keeps its own gate and checks.

## Example
Initial state: the kit is at `~/.swarm/kit`, and a step holds `plan-review`.
Action: read `<kit root>/blocks/plan-review/scripts/start_plan.py` as `~/.swarm/kit/blocks/plan-review/scripts/start_plan.py`.
Result: the path resolves from the kit root, not from the run folder.

## Check (script)
- none: each part's script check names its own script under the kit root.

## Check (judge)
- none: each part's judge checks read its own plan or round files.
