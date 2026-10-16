---
name: ui-motion
description: "Pick one timing owner for an interface's durations and curves, and set its animated properties and reduced-motion form. Use when the states of a page or app animate."
license: MIT
metadata:
  kind: rule
  owns: "UI timing selection, animated properties, and reduced motion"
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
- Animate `transform` and `opacity`. Animate another property only where the timing owner allows it for that case; the emil pack allows `clip-path` and accordion height. Never `transition: all`. Static content needs no animation.
- Honor `prefers-reduced-motion: reduce`. Keep opacity fades that help the reader, and remove movement. Loops and parallax become static.
- Keep every key fact visible under reduced motion. The settled-state check belongs to `blocks/web-ui-basics/SKILL.md`.
- If the selected owner's rows disagree for this state, use `blocks/decision-with-default/SKILL.md` to name the conflict before building it.
- Story motion follows ink's `skills/ink/references/story-and-motion.md`.

## Example
Initial state: a settings page opens a dropdown, and the code has no motion tokens.
Action: name the `animate` tables as the owner, and take 180 ms with `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)` from them. Under reduced motion, the dropdown fades with no slide.
Result: the motion list has one row, the dropdown, with its duration and curve from the timing owner and its reduced-motion form from this block.

## Check (script)
- none: the judge checks each state against its timing owner and this block's property and reduced-motion rules.

## Check (judge)
- Does the step name one timing owner, and does each duration and curve follow it? Quote the owner and each state with its source lines.
- Does each animation use only the properties this block allows? Quote its property list and the code.
- Does each animated state follow this block's reduced-motion form and keep every key fact visible? Quote the source rule and the reduced-motion shot; for static content, quote its visible facts.
