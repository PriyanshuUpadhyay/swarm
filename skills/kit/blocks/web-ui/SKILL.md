---
name: web-ui
description: "Keep the run-location rule that the web page blocks share with each selected page part. Use when a step builds, checks, shoots, or animates a web page."
license: MIT
metadata:
  kind: parent
  owns: "shared web page rules"
  serves: "design, build, review"
  needs: "a page or its code"
  gives: "shared rules for the selected web page parts"
  conflicts: ""
  users: "implement, build, planner"
  source: "blocks/web-ui-basics/SKILL.md, blocks/design-tokens/SKILL.md, and blocks/look-check/SKILL.md (each: run from the repo root when you use this block's script checks)"
  parts: "web-ui-basics, ui-motion, design-tokens, headless-shot, look-check"
---

## Rules
- Run from the repo root when you use a block's script checks.

Each part keeps its own checks.

## Example
Initial state: a step holds `design-tokens` and `web-ui-basics`, and the page is in `site/`.
Action: from the repo root, run the lint of `design-tokens` and the `rg` scans of `web-ui-basics`.
Result: each command finds `site/DESIGN.md` and the page files. Run from inside `site/`, the same commands would find no file.

## Check (script)
- none: each part's script checks hold their own repo-relative paths.

## Check (judge)
- none: each part's judge checks read its own shots and code.
