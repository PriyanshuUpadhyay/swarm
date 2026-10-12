# style-frames-concept
Concept: Prove the look before the full build with frames rendered from the real page code, scored beside the reference; a miss blocks.
Serves: design
Source: skills/ink/SKILL.md:55-57 (step 4); skills/ink/references/site.md:69-73 (site frames); skills/ink/references/qa.md:28-47 (reference match, rounds), :128-129 (visually verified)

## Rules
- Render three frames from the real page code with the screenshot tool the workflow names: the hero at 1440 px and at 390 px as `<page>-1440.png` and `<page>-390.png`, and one middle section at 1440 px as `<section>-1440.png`. Save them in `{{shots}}`, with one `compare-NN.png` per frame beside the reference.
- Score every line of the look grammar as `match`, `partial`, or `miss`, each with pixel evidence. Compare one crop of the whole frame and one crop at 100 % zoom.
- A `miss` on any trait blocks the build step.
- Do at least two rounds of fix and re-render. Record the scores of each round in `02-design.md`.
- After round 3 with the same `miss`, stop and show the user the frame and the miss. Write `accepted miss: <trait>, user: <their words>` only after the user has seen the frame. Later rounds still score every trait. A miss that changes asks again.
- Never write "visually verified" unless you inspected pixels of the final page after the last fix.

## Example
Initial state: the site's `index.html` has the hero in real HTML and CSS, and `DESIGN.md` exists.
Action: render `{{shots}}/home-1440.png`, `home-390.png`, and `flow-1440.png`, and make `compare-01.png` to `compare-03.png`, one per frame, beside the reference. Round 1: palette `match`, type scale `partial` (headline 3 lines at 390 px), density `miss` (twice the reference's whitespace). Fix the tokens and re-render. Round 2: all `match`.
Result: `02-design.md` holds both score tables and the compare image paths. The build may start.

## Check (script)
- `ls {{shots}}/*-1440.png | wc -l` prints 2 or more, `ls {{shots}}/*-390.png` exits 0, and `ls {{shots}}/compare-*.png | wc -l` prints 3 or more: the frames and one compare image per frame exist.

## Check (judge)
- Does `02-design.md` record two or more rounds, each with a score table, one row per grammar line, each with pixel evidence? Quote each round's table.
- Does the last round have no `miss`, or does each remaining one have an `accepted miss:` line with the user's words? Quote them.
