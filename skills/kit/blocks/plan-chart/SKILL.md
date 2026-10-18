---
name: plan-chart
description: "Check a plan file and draw it as a Mermaid flowchart in plan.md and plan.html: stages top to bottom, parallel steps side by side, arrows from each Needs, and the dropped blocks. Use when a plan is written or changed."
license: MIT
metadata:
  kind: step
  owns: "the check and the drawing of a plan's step graph"
  serves: "plan"
  needs: "plan.md"
  gives: "the chart in plan.md, and plan.html, or the first defect"
  conflicts: ""
  users: "plan"
  source: "blocks/html-diagram/SKILL.md (HTML boxes that reflow); docs/progress.md B3 (refuse an unknown Needs word or a cycle); references/step_run.py (need_files); assets/pele.global.js is npm pele 1.6.0 dist/pele.global.js (https://pele.run, MIT, assets/pele.LICENSE, sha256 b82d1bcd435302779a945e1e0f4a54ea56fb26e010ce3f2122f4216fcfc095d7)"
---

## Rules
- Run `python3 -I <kit root>/blocks/plan-chart/scripts/plan_chart.py <run>/plan.md`.
- On exit 1, the script prints the first defect: an unknown block, a Needs word that names no step, a loop, two conflicting blocks, a step whose output no step uses, an end with no `voice-` block, a blocks table that misses a catalog block or disagrees with the steps, or a `Done when:` part with no claim bullet, no `Evidence:` bullet, or a line that is not a bullet. Fix `plan.md` and run it again.
- On exit 0, it writes one Mermaid `flowchart TD` into `plan.md`, in a `mermaid` fence between `<!-- plan-chart:start -->` and `<!-- plan-chart:end -->` under the title. It replaces the old fence, so a second run with no plan change writes the same bytes. Plannotator draws the fence, and the approved `plan.md` holds the chart that the owner saw.
- The chart has a subgraph per stage, labeled "N · <step name>", or "N · parallel" when the stage holds more than one step. Each step node shows its id, then its blocks. Arrows come from the Needs. A dashed "Dropped blocks" subgraph holds one node per Rejected block with its reason cut to 8 words. Each label is escaped as Mermaid `#code;`, so no plan text can break the graph.
- It also writes `plan.html` beside `plan.md`. The page draws the same Mermaid text with the Pele build in `assets/pele.global.js`, inlined, so it works offline from `file://`. Below the chart, each stage is a row of step boxes. Each box shows its blocks, what it gets from which step, what it gives, and why. The page shows `Done when:` as a result line, a claim list, and an evidence list, and shows `code` spans as code.
- "Blocks considered" lists the Chosen and Rejected blocks, one row per block, with the reason cut to 12 words. A part sits right under its parent, as `↳ part`, or as `parent ↳ part` when the parent has the other status. The script also prints these as markdown tables after the path. `planner` owns the rule for the table and for the reply that shows it.
- Never edit `plan.html` or the fence between the markers by hand. Run the script again after every change to `plan.md`, so the chart never shows an old plan. A chart change is a change to `plan.md`; `plan-review` owns what that does to an approval.

## Example
Initial state: `plan.md` has 6 steps, and 04-build needs `read, tokens`.
Action: run the script.
Result: `plan.md` gets a chart with 5 stage subgraphs. Subgraph "2 · parallel" holds 02-read and 03-tokens, and both have an arrow to 04-build. `plan.html` draws the same chart, and its 04-build box says "Gets: findings with paths from 02-read; DESIGN.md tokens from 03-tokens".

## Check (script)
- `python3 -I <kit root>/blocks/plan-chart/scripts/test_plan_chart.py` prints `ok`: a good plan draws and runs as a step run; the Mermaid text has the stage subgraphs, the parallel label, the arrows, and the dropped group; a hostile label is escaped and the pinned Pele renders it; a second run leaves `plan.md` the same and a changed plan replaces the one fence; `start_plan.py` refuses after a chart change; the tables have one row per block with parts under their parent and short reasons; the page shows the Done-when lists and code spans; and each defect stops the script.

## Check (judge)
- none: the script owns every check of the chart.
