---
name: look-check
description: "Check the look before the full build: render a few frames from the real code and score them beside the reference; a miss blocks the build. Use when a page or piece must match a reference look."
license: MIT
metadata:
  kind: step
  owns: "the proof of the look before the full build"
  serves: "design"
  needs: "tokens and page code"
  gives: "frames scored beside the reference"
  conflicts: ""
  users: "implement, build, build-site"
  source: "skills/ink/SKILL.md (step 4); skills/ink/references/site.md (site frames, moved to blocks/build-site 2026-10-13); skills/ink/references/qa.md (reference match and rounds, moved 2026-10-13)"
---

## Rules
- Capture and name frames and compare images by `blocks/headless-shot/SKILL.md`. For a page, render three: the hero at 1440 px and at 390 px and one middle section at 1440 px. For a reel, one of the frames is the signature shot.
- Inspect the built frames by `blocks/visual-qa/SKILL.md`. Score every line of the look (the `DESIGN.md` tokens, or the list of look traits the work keeps) as `match`, `partial`, or `miss`, each with pixel evidence.
- A `miss` on any trait blocks the build step.
- Do at least two rounds of fix and re-render, even when round 1 has no miss. Record the scores of each round in the step's own file.
- After round 3 with the same `miss`, stop and show the user the frame and the miss. Write `accepted miss: <trait>, user: <their words>` only after the user has seen the frame. Later rounds still score every trait and exempt only that accepted miss from the gate. A miss that changes asks again.

## Example
Initial state: the site's `index.html` has the hero in real HTML and CSS, and `DESIGN.md` exists.
Action: render `{{shots}}/home-1440.png`, `home-390.png`, and `flow-1440.png`, and make `compare-01.png` to `compare-03.png`, one per frame, beside the reference. Round 1: palette `match`, type scale `partial` (headline 3 lines at 390 px), density `miss` (twice the reference's whitespace). Fix the tokens and re-render. Round 2: all `match`.
Result: the step file holds both score tables and the compare image paths. The build may start.

## Check (script)
- `ls {{shots}}/compare-*.png | wc -l` prints 1 or more: a compare image exists.

## Check (judge)
- For a page, do the two hero frames and the middle-section frame exist, named by `blocks/headless-shot/SKILL.md`; for a reel, is one frame the signature shot? Quote the paths.
- Does each frame have its own `compare-*.png` in {{shots}}, so three or more for a page? Quote the paths.
- Does the step file record two or more rounds, each with a score table, one row per grammar line, each with pixel evidence? Quote each round's table.
- Does the last round have no `miss`, or does each remaining one have an `accepted miss:` line with the user's words? Quote them.
- Does any frame show a default that `blocks/design-tokens/SKILL.md` names? Quote the frame and the default, or write "none".
