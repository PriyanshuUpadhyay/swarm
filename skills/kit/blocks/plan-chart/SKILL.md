---
name: plan-chart
description: "Check a plan file and draw it as a Mermaid flowchart in plan.md and plan.html: steps top to bottom, parallel steps side by side, and arrows from each Needs. Use when a plan changes."
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
- On exit 1, the script prints the first defect: an unknown block, a Needs word that names no step, a loop, two conflicting blocks, a step whose output no step uses, an end with no `voice-` block, a step table with no Writes column (only a plan that equals its `Stop: chat` preset may leave it out), two parallel steps whose Writes overlap, an empty Writes cell or `none` beside a path, a step with Needs `none` and no `requirements`, a writing step that no `change-review` step reaches through Needs, a blocks table that disagrees with the steps or has a row that is neither Chosen nor Offered (an Offered row names a block that no step loads), or a `Done when:` part with no claim bullet, no `Evidence:` bullet, or a line that is not a bullet. Fix `plan.md` and run it again.
- On exit 0, it writes one Mermaid `flowchart TD` into `plan.md`, in a `mermaid` fence between `<!-- plan-chart:start -->` and `<!-- plan-chart:end -->` under the title. It replaces the old fence, so a second run with no plan change writes the same bytes. Plannotator draws the fence, and the approved `plan.md` holds the chart that the owner saw.
- The chart shows only the flow. Each step node shows its id, then its blocks, and arrows come from the Needs. A stage with two or more steps is a subgraph labeled "N · parallel". A one-step stage is a plain node. Each label is escaped as Mermaid `#code;`, so no plan text can break the graph.
- It also writes `plan.html` beside `plan.md`. The page draws the same Mermaid text with the Pele build in `assets/pele.global.js`, inlined, so it works offline from `file://`. Below that, each stage is a row of step boxes. Each box shows its blocks, what it gets from which step, what it gives, and why. The page shows `Done when:` as a result line, a claim list, and an evidence list, and shows `code` spans as code.
- "Blocks considered" lists the Chosen blocks, then the Offered ones when the plan has any, one row per block, with the reason cut to 12 words. A part sits right under its parent, as `↳ part`. The script also prints the table as markdown after the path. `planner` owns the rule for the table and for the reply that shows it.
- Never edit `plan.html` or the fence between the markers by hand. Run the script again after every change to `plan.md`, so the chart never shows an old plan. A chart change is a change to `plan.md`; `plan-review` owns what that does to an approval.

## Example
Initial state: `plan.md` has 6 steps, and 04-build needs `read, tokens`.
Action: run the script.
Result: `plan.md` gets a chart with one subgraph. Subgraph "2 · parallel" holds 02-read and 03-tokens, and both have an arrow to 04-build. `plan.html` draws the same chart, and its 04-build box says "Gets: findings with paths from 02-read; DESIGN.md tokens from 03-tokens".

## Check (script)
- `python3 -I <kit root>/blocks/plan-chart/scripts/test_plan_chart.py` prints `ok`: a good plan draws and runs as a step run; the Mermaid text has plain nodes for one-step stages, the parallel subgraph, and the arrows; Writes overlap by path parts; a hostile label is escaped and the pinned Pele renders it; a second run leaves `plan.md` the same and a changed plan replaces the one fence; `start_plan.py` refuses after a chart change; the tables have one row per block with parts under their parent and short reasons; the page shows the Done-when lists and code spans; and each defect stops the script.

## Check (judge)
- none: the script owns every check of the chart.
