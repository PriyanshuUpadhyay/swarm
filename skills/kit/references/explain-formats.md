---
description: Owner rule for when an explanation adds a diagram or a local HTML page to its text. Read when a skill or AGENTS.md points here, before you write a map, a report, a plan, or a reply that explains how something works.
---

# Explain formats

Text in the reply is the default and always carries the whole result. A diagram or a page is added
only when it makes the result easier to understand than text alone.

Example. A change moves a login check from the API handler to a middleware that three routes share.
In text, the reader must hold four names and their order. A five-line diagram shows the request
passing the middleware before each route, and the reader sees the new order at once.

## Diagram

Draw one when the subject does at least one of these:

- calls across three or more parts in an order that matters;
- adds, removes, or reorders a branch, a guard, or an async step;
- moves through a lifecycle, a status, or a retry flow;
- changes a schema or a data model.

Skip it for one file, a rename, a config value, or a list. The text already shows those.

Pick the type from the shape. Use a sequence for calls over time, a flowchart for decisions, a state
diagram for a lifecycle, and an entity diagram for a schema. Every branch leads somewhere, so the
diagram also shows the edge cases.

The format depends on where it is read:

- In a reply or a terminal, draw a text diagram with boxes and arrows in a fenced block, because a
  terminal does not render Mermaid.
- In a file that is read on GitHub or in a browser, use Mermaid (`sequenceDiagram`, `flowchart`,
  `stateDiagram-v2`, `erDiagram`).

A node or an arrow that names code carries its `path:line`, and the caller's own check covers those
citations.

## Local HTML page

Write one self-contained HTML file next to the result when the reader must compare or explore more
than text and one diagram can hold. Examples are a table that needs sorting or filters, more than
one linked diagram, a timeline, or a before-and-after view. Put the page in the run folder, render
its Mermaid with the library from `cdn.jsdelivr.net`, and give its path in one line of the reply.

The page is extra. The reply still carries the whole result, and the agent never publishes the page
or opens it.

A walk is the one exception. A walk from the borrowed `glitch-walk` skill
(https://github.com/Glitch-Cat-Club/glitch-skills) shows one action as real screens, each with the
real code lines behind it, and text cannot hold that. So its page is the main result. The reply gives
the page's path and two or three lines on what it covers and what was not checked. The agent still
never publishes or opens the page.

## Not yet

An explainer video is an open item. It waits for a test of the tools against the `ink` skill.
