---
description: Owner rule for the visual form (table, text diagram, or local HTML page) of a report, plan, review, status, or explanation with more than three facts. Read when a skill or AGENTS.md points here, before you write a map, a report, a plan, a status, or a reply that explains how something works.
---

# Explain formats

Owner rule for the form of a report, plan, review, status, or explanation. A result with more than
three facts carries a visual form: a table, a text diagram, or a local HTML page. The reply always
carries the whole result, so the page is extra and never the only copy. Prose alone is for a result
of three facts or fewer.

| Content shape | Form |
|---|---|
| calls over time, across three or more parts | sequence diagram |
| decisions, branches, or a retry flow | flowchart |
| dates, order, or phases | timeline |
| a lifecycle or a status | state diagram |
| many items, each with the same attributes | table |
| a schema or a data model | entity diagram |
| more than one screen of linked diagrams, or a table that needs sorting | local HTML page |

Before. A status reply says: "Task 1 is done and checked. Task 2 is blocked on a question about the
retry limit. Task 3 is running and has 2 commits. Task 4 has not started."

After. The reader sees the state of all four at once.

| Task | State | Evidence or blocker |
|---|---|---|
| 1 | ✅ done | checks pass |
| 2 | ❓ blocked | retry limit: owner decides |
| 3 | running | 2 commits |
| 4 | ➖ not started | waits for task 3 |

## Diagram

Draw one when the subject does at least one of these:

- calls across three or more parts in an order that matters;
- adds, removes, or reorders a branch, a guard, or an async step;
- moves through a lifecycle, a status, or a retry flow;
- changes a schema or a data model.

Skip it for one file, a rename, a config value, or a list of three items or fewer. The text already
shows those.

Pick the type from the table above. Every branch leads somewhere, so the diagram also shows the
edge cases.

The format depends on where it is read:

- In a reply or a terminal, draw a text diagram with boxes and arrows in a fenced block, because a
  terminal does not render Mermaid.
- In a file that is read on GitHub or in a browser, use Mermaid (`sequenceDiagram`, `flowchart`,
  `stateDiagram-v2`, `erDiagram`, `timeline`).

A node or an arrow that names code carries its `path:line`, and the caller's own check covers those
citations.

## Local HTML page

Write one self-contained HTML file next to the result when the reader must compare or explore more
than a table and one text diagram can hold. Examples are a table that needs sorting or filters, more
than one linked diagram, a long timeline, or a before-and-after view. Put the page in the run folder, render
its Mermaid with the library from `cdn.jsdelivr.net`, and give its path in one line of the reply.

The page is extra. The reply still carries the whole result, and the agent never publishes the page
or opens it.

A walk is the one exception. A walk from the borrowed `glitch-walk` skill
(https://github.com/Glitch-Cat-Club/glitch-skills) shows one action as real screens, each with the
real code lines behind it, and text cannot hold that. So its page is the main result. The reply gives
the page's path and two or three lines on what it covers and what was not checked. The agent still
never publishes or opens the page. Its diagram is the step map that the skill draws from the walk in
HTML and CSS, not Mermaid, so the page needs no library; `blocks/html-diagram/SKILL.md` (from the kit root, one folder up from this file) owns how that map
is drawn.

## Not yet

An explainer video is an open item. It waits for a test of the tools against the `ink` skill.
