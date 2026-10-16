---
name: diagram
description: "Keep shared diagram rules with each selected part. Use when a step chooses, draws, or checks a diagram."
license: MIT
metadata:
  kind: parent
  owns: "shared diagram rules"
  serves: "design, build, impact, review"
  needs: "a subject or a diagram"
  gives: "shared rules for the selected diagram parts"
  conflicts: ""
  users: "implement"
  source: "blocks/when-to-diagram/SKILL.md (citations, caption, branch check); blocks/html-diagram/SKILL.md (caption and text description); blocks/web-ui-basics/SKILL.md (text description); blocks/call-tree/SKILL.md (citations); blocks/diagram-check/SKILL.md (branch coverage and caption); references/explain-formats.md (shared rules from the Diagram section); html-plan pack: skills/html-plan/SKILL.md:119 (caption)"
  parts: "when-to-diagram, html-diagram, call-tree, diagram-check"
---

## Rules
- Every branch in the source appears in the diagram and ends at a node or a final state.
- A node or an arrow that names code carries its `path:line`. In an HTML page, put it in the source as a comment or `data-src`. The caller's own check covers these citations.
- A diagram shown in a page, file, or reply has a caption. Each caption is one sentence that says what to notice. A plain-text call tree needs no caption. In a page, give the diagram a text description.

`references/explain-formats.md` owns when to draw, the diagram type, and the form for the reading surface.
Each part keeps its own size, layout, record, or review checks.

## Example
Initial state: `src/collector.rs:40` retries a failed send three times, then drops and logs the span.
Action: draw a collector node with `data-src="src/collector.rs:40"` and a `retry x3` branch that ends at "dropped, logged".
Add the caption "A failed send gets three retries before the span is dropped and logged."
On a page, add the text description "Collector retries a failed send three times, then drops and logs the span."
Result: the source branch appears, its end is clear, and the node has its citation, caption, and page text description.

## Check (script)
- none: branches, citations, and caption meaning need the caller's diagram and source.

## Check (judge)
- For each diagram that the run draws, does every source branch appear and end at a node or a final state? Quote each source branch, its diagram branch, and its end, or write `no diagram drawn`.
- For each diagram that the run draws, does each code name have its source citation? Quote the names and citations, or write `no diagram drawn`.
- For each diagram that the run draws, except a plain-text call tree, is its caption one sentence that says what to notice? Does each page diagram have its text description? Quote them, mark a call tree's caption `not needed`, or write `no diagram drawn`.
