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
  users: "flow"
  source: "blocks/when-to-diagram/SKILL.md (citations, caption, branch check); blocks/html-diagram/SKILL.md (caption and text alternative); blocks/web-ui-basics/SKILL.md (text description); blocks/call-tree/SKILL.md (citations); blocks/diagram-check/SKILL.md (branch coverage and caption)"
  parts: "when-to-diagram, html-diagram, call-tree, diagram-check"
---

## Rules
- Every branch ends at a node or a final state, including the edge cases from the source.
- A node or an arrow that names code carries its `path:line`. In an HTML page, put it in the source as a comment or `data-src`. The caller's own check covers these citations.
- The caption is one sentence that says what to notice. In a page, give the diagram a text description.

`references/explain-formats.md` owns when to draw, the diagram type, and the form for the reading surface.
Each part keeps its own size, layout, record, or review checks.

## Example
Initial state: a design step selects `when-to-diagram`, and a build step selects `html-diagram`.
Action: flow adds this parent's rules once before the selected part in each step.
Result: both steps get the shared rules, and each keeps only its own part's checks.

## Check (script)
- none: branches, citations, and caption meaning need the caller's diagram and source.

## Check (judge)
- For each diagram that this step draws, does each branch reach a node or a final state? Quote the branches and their ends, or say that this step only chooses the diagram.
- For each diagram that this step draws, does each code name have its source citation? Quote the names and citations, or say that this step only chooses the diagram.
- For each diagram that this step draws, is its caption one sentence that says what to notice, and does a page diagram have its text description? Quote them, or say that this step only chooses the diagram.
