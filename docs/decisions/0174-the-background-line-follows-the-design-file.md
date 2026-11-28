---
status: accepted
date: 2026-11-27
deciders: [user]
amends: ["0104"]
related: ["0164"]
informed-by:
  - 'chair: "one new ADR line for the background line"'
  - 'recommendation: "yes"'
  - 'owner: "I accept your recommendation."'
---

# 0174. The background line follows the design file

The background line in [the design file](../design/components.md) replaces the
"Running · <label>" line in the 2026-11-11 amendment to ADR 0104.

The line shows "In background" with counts for agents, shells, and failures, a timer for running
work with a known start, and Open to jump to its row. It shares the ⌘J panel's reads from
`/api/background` and `/api/shells` and uses the same session snapshot for agents.
