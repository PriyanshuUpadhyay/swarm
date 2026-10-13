---
name: ui-motion
description: "Pick one timing owner for an interface's motion and take every duration, curve, and reduced-motion rule from it. Use when the states of a page or app animate."
license: MIT
metadata:
  kind: rule
  owns: "UI timing, easing, and reduced motion, from one timing owner"
  serves: "build, review"
  needs: "the UI states that change"
  gives: "a motion list with its timing owner"
  conflicts: ""
  users: "web-ui-basics, build-site, planner"
  source: "emilkowalski pack: skills/animate/SKILL.md (easing and duration tables, reduced motion); blocks/web-ui-basics/SKILL.md (Motion, moved here 2026-10-13); docs/progress.md B8"
---

## Rules
- The kit root is two folders up from this file.
- One timing owner per interface. When the code has a duration scale or easing tokens, they are the owner, and you extend them. Otherwise the owner is the tables of `<kit root>/vendor/emil-skills/skills/animate/SKILL.md`. Name the owner in the step's file.
- Take every duration and curve from the owner. Never invent a value.
- Animate only `transform` and `opacity`. Never `transition: all`.
- UI animations stay under 300 ms. Use ease-out to enter. Never ease-in.
- Honor `prefers-reduced-motion: reduce`: fewer and gentler, not zero. Keep opacity fades, and remove movement. Loops and parallax become static.
- Story motion (the beats, shots, and transitions of a reel or a scroll story) belongs to the `story` block, not here.

## Example
Initial state: a settings page opens a dropdown, and the code has no motion tokens.
Action: name the `animate` tables as the owner, and take 180 ms with `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)` from them. Under reduced motion, the dropdown fades with no slide.
Result: the motion list has one row, the dropdown, with its duration, curve, and reduced-motion form, each from the owner.

## Check (script)
- none: the `web-ui-basics` block runs the script checks for the banned motion forms and for reduced motion.

## Check (judge)
- Does the step name one timing owner, and does each duration and curve in the code come from it? Quote the owner and two values with their source lines.
