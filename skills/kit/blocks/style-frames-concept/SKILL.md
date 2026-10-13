---
name: style-frames-concept
description: "Prove the look before the full build with frames rendered from the real page code, scored beside the reference; a miss blocks."
license: MIT
metadata:
  kind: step
  owns: "the proof of the look before the full build"
  serves: "design"
  needs: "tokens and page code"
  gives: "frames scored beside the reference"
  conflicts: ""
  users: "flow, ink, build-site"
  source: "skills/ink/SKILL.md (step 4); skills/ink/references/site.md (site frames, moved to blocks/build-site 2026-10-13); skills/ink/references/qa.md (reference match and rounds, moved 2026-10-13)"
---

## Rules
- Render the frames from the real code with the screenshot tool the workflow names, and save them in `{{shots}}`, with one `compare-NN.png` per frame beside the reference at the same scale. For a page, render three: the hero at 1440 px and at 390 px as `<page>-1440.png` and `<page>-390.png`, and one middle section at 1440 px as `<section>-1440.png`. For a reel, one of the frames is the signature shot.
- Score every line of the look grammar as `match`, `partial`, or `miss`, each with pixel evidence. Compare one crop of the whole frame and one crop at 100 % zoom, because texture and line swell show only at full size.
- A `miss` on any trait blocks the build step.
- Do at least two rounds of fix and re-render. Record the scores of each round in the step's own file.
- After round 3 with the same `miss`, stop and show the user the frame and the miss. Write `accepted miss: <trait>, user: <their words>` only after the user has seen the frame. Later rounds still score every trait and exempt only that accepted miss from the gate. A miss that changes asks again.

## Example
Initial state: the site's `index.html` has the hero in real HTML and CSS, and `DESIGN.md` exists.
Action: render `{{shots}}/home-1440.png`, `home-390.png`, and `flow-1440.png`, and make `compare-01.png` to `compare-03.png`, one per frame, beside the reference. Round 1: palette `match`, type scale `partial` (headline 3 lines at 390 px), density `miss` (twice the reference's whitespace). Fix the tokens and re-render. Round 2: all `match`.
Result: the step file holds both score tables and the compare image paths. The build may start.

## Check (script)
- `ls {{shots}}/compare-*.png | wc -l` prints 1 or more: a compare image exists.

## Check (judge)
- For a page, do `<page>-1440.png`, `<page>-390.png`, and one `<section>-1440.png` exist in {{shots}}; for a reel, is one frame the signature shot? Quote the paths.
- Does each frame have its own `compare-*.png` in {{shots}}, so three or more for a page? Quote the paths.
- Does the step file record two or more rounds, each with a score table, one row per grammar line, each with pixel evidence? Quote each round's table.
- Does the last round have no `miss`, or does each remaining one have an `accepted miss:` line with the user's words? Quote them.
