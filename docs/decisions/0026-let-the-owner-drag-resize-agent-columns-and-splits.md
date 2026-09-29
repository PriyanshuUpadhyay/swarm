---
status: accepted
date: 2026-09-29
deciders: [user]
related: ["0022"]
informed-by:
  - "User answer on 2026-09-29: drag resize will be awesome"
---

# 0026. Let the owner drag-resize agent columns and splits

In the context of the pane strip (ADR 0022), facing fixed column widths (1/3 of the main area,
at least 440 pt) and a fixed 50/50 split inside a two-pane column, we chose to let the owner drag
a column's edge to set the width of all agent columns (440 pt to 90% of the main area) and drag
the line inside a two-pane column to set its split (25% to 75%), both kept across launches and
reset by a double-click, and neglected per-column widths and a free grid, to let the owner fit
more or fewer agents on screen, accepting one more saved setting and divider hit areas that must
not steal clicks from the terminals.
