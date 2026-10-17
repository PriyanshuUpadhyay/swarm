---
name: diagram-check
description: "Check that each diagram tells the truth about its source, invents nothing, and reads on a phone. Use when you review a page or plan that has a diagram."
license: MIT
metadata:
  kind: rule
  owns: "the review of a diagram against its source and at phone width"
  serves: "review"
  needs: "a diagram and its source"
  gives: "pass or miss per diagram"
  conflicts: ""
  users: "implement"
  source: "references/explain-formats.md (Local HTML page section); html-plan pack: skills/html-plan/SKILL.md:82 (rule 8, real over drawn), :130 (no clipping, arrows do not cross labels); blocks/visual-qa/SKILL.md (lettering, from build's qa.md); blocks/story/SKILL.md (each key frame reads on its own, from build's qa.md); web-interface-guidelines pack: command.md:168-170 (finding format)"
---

## Rules
- Run from the repo root when you use this block's script checks.
- The source of truth is {{source_of_truth}}. First name the part of it that each diagram draws, with its path.
- Map each node and each arrow to one source line. Write one row per item: item, source `path:line`, `match` or `miss`.
- A node or an arrow with no source line is an invented component. It is a `miss`.
- Real over drawn: names on the diagram use the same words as the source.
- Shoot each diagram at each width from `blocks/web-ui-basics/SKILL.md` to `{{shots}}/diagram-<diagram>-<W>.png`, where W is that width. A page shot cannot stand in for it. Read each label in it at 100 % zoom. No label is clipped, and no arrow crosses a label. A moving part never hides a letter.
- The diagram reads on its own: a reader gets the caption's point from the picture without the body text.
- Write each finding as `path:line - issue`, grouped by file.

## Example
Initial state: the request-flow diagram has nodes CLI, agent, collector, store, and cache. The source is `docs/architecture.md:10-24`.
Action: build the table. CLI `:11 match`, agent `:13 match`, collector `:15 match`, store `:18 match`, cache: no line, `miss`. The arrow "retry x3" matches `:21`. The shot `{{shots}}/diagram-request-flow-<W>.png`: the "collector" label overlaps the retry arrow.
Result: two findings, `site/index.html:88 - node "cache" not in docs/architecture.md` and `site/index.html:95 - retry arrow crosses "collector" label at the checked width`.

## Check (script)
- `file {{shots}}/diagram-*.png` prints `PNG image data, W x` on every line, where W matches the filename and is a width from `blocks/web-ui-basics/SKILL.md`: each diagram shot exists at its named check width.

## Check (judge)
- Does each diagram on the page have its own `{{shots}}/diagram-<diagram>-<W>.png` shot at each width from `blocks/web-ui-basics/SKILL.md`? Quote each diagram's caption and its shot name.
- Does each source `path:line` in the tables hold the item its row maps? Quote each row and its source line.
- Does any node, arrow, or label say something that {{source_of_truth}} does not say? Quote the diagram text and the source line, or write "no source line".
- Does the review step's file (`06-review.md` in implement) hold one table per diagram, with one row per node and per arrow? Quote one table.
- Is each `miss` row also a finding in the form `path:line - issue`? Quote each miss and its finding.
