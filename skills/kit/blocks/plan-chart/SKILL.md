---
name: plan-chart
description: "Check a plan file and draw it as an HTML flowchart: stages top to bottom, parallel steps side by side in a stage, and what each step gets from which step. Use when a plan is written or changed."
license: MIT
metadata:
  kind: step
  owns: "the check and the drawing of a plan's step graph"
  serves: "plan"
  needs: "plan.md"
  gives: "plan.html, or the first defect"
  conflicts: ""
  users: "plan"
  source: "blocks/html-diagram/SKILL.md (HTML boxes that reflow); docs/progress.md B3 (refuse an unknown Needs word or a cycle); references/step_run.py (need_files)"
---

## Rules
- Run `python3 -I <kit root>/blocks/plan-chart/scripts/plan_chart.py <run>/plan.md`. The kit root is two folders up from this file.
- On exit 1, the script prints the first defect: an unknown block, a Needs word that names no step, a loop, two conflicting blocks, a step whose output no step uses, an end with no `voice-` block, or a blocks table that misses a catalog block or disagrees with the steps, or a `Done when:` with no claim bullet or no `Evidence:` bullet. Fix `plan.md` and run it again. Never edit `plan.html` by hand.
- On exit 0, it writes `plan.html` beside `plan.md`. Each stage is a row under the one before it, and the steps in one stage run in parallel and wrap to fit the width. The page shows `Done when:` as a result line, a claim list, and an evidence list, and shows `code` spans as code. Each step box shows its blocks, what it gets from which step, what it gives, and why. Below the chart, "Blocks considered" lists the Chosen and Rejected blocks with their reasons, and sub-blocks under their alias. The script also prints these as markdown tables after the path. `planner` owns the rule for the table and for the reply that shows it.
- Run it again after every change to `plan.md`, so the chart never shows an old plan.

## Example
Initial state: `plan.md` has 6 steps, and 04-build needs `read, tokens`.
Action: run the script.
Result: `plan.html` has 5 stages. Stage 2 says "2 steps run in parallel" and holds 02-read and 03-tokens. The 04-build box says "Gets: findings with paths from 02-read; DESIGN.md tokens from 03-tokens".

## Check (script)
- `python3 -I <kit root>/blocks/plan-chart/scripts/test_plan_chart.py` prints `ok`: a good plan draws and runs as a step run, the page and the printed tables group Chosen and Rejected with sub-blocks under their alias, and each defect stops the script.

## Check (judge)
- none: the script owns every check of the chart.
