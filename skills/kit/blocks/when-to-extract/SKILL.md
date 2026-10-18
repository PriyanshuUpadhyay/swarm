---
name: when-to-extract
description: "Keep code inline unless a split has one of five reasons, so a reader holds fewer names and jumps. Use when you add, split, or keep a helper, layer, file, or module."
license: MIT
metadata:
  kind: rule
  owns: "when code gets its own function, file, or module"
  serves: "build, review"
  needs: "a change that adds, splits, or keeps a unit of code"
  gives: "the reason that holds for each extraction, or an inline"
  conflicts: ""
  users: "pair, review-check, cleanup-gate"
  source: "blocks/when-to-extract/SKILL.md (rule moved from its former skill on 2026-10-13)"
---

## Rules
- A split must cut the number of ideas a reader holds. Fewer lines per function is not the goal. Watch two costs: the layers to trace (jumps between the question and the answer) and the state to hold (hidden or changing context).
- Keep code inline by default. Give it its own function, file, or module only when at least one of these is true:
  1. It is reused. Two or more current callers use it.
  2. It has a clear domain name. The name is an idea of the domain, not a step such as `process_part_2` or `handle_data`.
  3. It hides real complexity. A caller can use it without reading its body.
  4. It has behavior that needs its own test.
  5. It sets a real boundary, such as a network, device, or storage edge, a trust boundary, or a public API.
- "This function or file is getting long" alone is not a reason.
- The test also works in reverse. A helper with one caller that meets no reason goes back inline. A pass-through layer that repeats the same methods and arguments collapses.
- Shrink state the same way. Prefer a returned value over a mutation, a local over a field, a field over module state, and module state over a global. Derive a value instead of syncing two copies. Name an invariant once, at the boundary, not in every consumer.
- The test: a new reader answers "where does X come from?" and "what can change X?" in under 30 seconds. If not, cut layers or cut state.

## Example
Initial state: a 60-line `import_orders` reads a CSV, checks each row, and writes the rows.
Action: a change splits it into `_read_rows`, `_check_row`, and `_write_rows`, each with one caller and straight-line code.
Result: no reason holds, so the reader now tracks four names and three jumps for the same flow, and the split is a defect. If the API handler also calls `_check_row`, reason 1 holds and that one extraction stays.

## Check (script)
- none: whether a split has a reason needs meaning, so its checks are judge checks.

## Check (judge)
- Does each new function, file, or module name the reason (1-5) that holds for it? Quote the unit and its reason, or write "no new unit".
- Does any new helper have one caller and no reason? Quote it, or write "none".
- Does any new layer only pass the same methods and arguments through? Quote it, or write "none".
