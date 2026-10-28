---
name: plan-chart
description: "Check a plan file, draw its step graph as a Mermaid flowchart in plan.md, and draw the whole plan as the page plan.html that the owner reviews. Use when a plan changes."
license: MIT
metadata:
  kind: step
  owns: "the check of a plan, the drawing of its step graph, and its review page"
  serves: "plan"
  needs: "plan.md"
  gives: "the chart in plan.md, and plan.html, or the first defect"
  conflicts: ""
  users: "plan"
  source: "blocks/html-diagram/SKILL.md (HTML boxes that reflow); docs/progress.md B3 (refuse an unknown Needs word or a cycle); references/step_run.py (need_files); assets/pele.global.js is npm pele 1.6.0 dist/pele.global.js (https://pele.run, MIT, assets/pele.LICENSE, sha256 b82d1bcd435302779a945e1e0f4a54ea56fb26e010ce3f2122f4216fcfc095d7)"
---

## Rules
- Run `python3 -I <kit root>/blocks/plan-chart/scripts/plan_chart.py <run>/plan.md`.
- On exit 1, the script prints the first defect: an unknown block, a Needs word that names no step, a loop, two conflicting blocks, a step whose output no step uses, an end with no `voice-` block, a step table with no Writes column (only a plan that equals its `Stop: chat` preset may leave it out), two parallel steps whose Writes overlap, an empty Writes cell or `none` beside a path, a step with Needs `none` and no `requirements`, a writing step that no `change-review` step reaches through Needs, a blocks table that disagrees with the steps or has a row that is neither Chosen nor Offered (an Offered row names a block that no step loads), or a `Done when:` part with no claim bullet, no `Evidence:` bullet, or a line that is not a bullet. A plan with `Preset: research` also stops on a middle step with no brief under `## Steps`, a `deep-search` brief with fewer than two seed searches, a `### Block` with no `So in this plan:` line, or a diagram with no `Answers:` line over it or more than 10 nodes. Fix `plan.md` and run it again.
- On exit 0, it writes one Mermaid `flowchart TD` into `plan.md`, in a `mermaid` fence between `<!-- plan-chart:start -->` and `<!-- plan-chart:end -->`, at the top of the `<details>` fold that holds the step table, or under the title when the plan has no fold. It replaces the old fence, so a second run with no plan change writes the same bytes. `plan.html` draws the fence with the inlined Pele build, and the approved `plan.md` holds the chart that the owner saw.
- The chart shows only the flow. Each step node shows its id, then its blocks, and arrows come from the Needs. A stage with two or more steps is a subgraph labeled "N · parallel". A one-step stage is a plain node. Each label is escaped as Mermaid `#code;`, so no plan text can break the graph.
- It also writes `plan.html` beside `plan.md`: the whole plan as one page, which the owner reviews alone. It has a sticky menu of the sections, a light and dark toggle, question cards, blue building-block cards with Known and To-check tags, green step cards with their step id and search chips, and the tables. Each Mermaid fence is drawn with the Pele build in `assets/pele.global.js`, inlined, so the page works offline from `file://`. An `html` fence goes in as written, for a diagram drawn by `blocks/html-diagram/SKILL.md`; it may use only plain tags (`div`, `span`, `figure`, lists, tables) with `class`, `style`, `id`, `title`, `role`, `aria-`, and `data-` attributes, and a `style` may use only the CSS functions `var`, `calc`, `min`, `max`, `clamp`, `rgb`, `hsl`, `repeat`, and `minmax`, with no `\` escape, `@`, or fixed position, so anything else stops the script. The page has no `<style>` for a fence, so a flow uses the page's `flow` class: an `<ol class="flow">` of `<li>` boxes, with → arrows that turn to ↓ in one column at 600 px and below. The page carries the hash of `plan.md` in a `plan-md-sha256` meta tag, which `start_plan.py` checks.
- The page's Skill blocks section comes from the blocks table: the used blocks with their description from each `SKILL.md`, the reason, and the steps that load them, a part right under its parent; then the offered blocks and why each is off; then every other kit block, folded. In a plan with `## Blocks considered`, it takes that section's place. `planner` owns the rule for the table.
- Never edit `plan.html` or the fence between the markers by hand. Run the script again after every change to `plan.md`, so the chart never shows an old plan. A chart change is a change to `plan.md`; `plan-review` owns what that does to an approval.

## Example
Initial state: `plan.md` has 6 steps, and 04-build needs `read, tokens`.
Action: run the script.
Result: `plan.md` gets a chart with one subgraph. Subgraph "2 · parallel" holds 02-read and 03-tokens, and both have an arrow to 04-build. `plan.html` draws the same chart inside its "How it runs" fold.

## Check (script)
- `python3 -I <kit root>/blocks/plan-chart/scripts/test_plan_chart.py` prints `ok`: a good plan draws and runs as a step run; the Mermaid text has plain nodes for one-step stages, the parallel subgraph, and the arrows; Writes overlap by path parts; a hostile label is escaped and the pinned Pele renders it; a second run leaves `plan.md` the same and a changed plan replaces the one fence; `start_plan.py` refuses after a chart change; the Skill blocks section has one row per block with its description and steps, parts under their parent; the page shows the Done-when lists and code spans; and each defect stops the script.
- `python3 -I <kit root>/blocks/plan-chart/scripts/test_plan_page.py` prints `ok`: the research example in `planner` draws its cards and chips, `step_run.py start` copies each brief into its step file, each research gap stops the script, and a fence inside a brief does not end it.

## Check (judge)
- none: the script owns every check of the chart.
