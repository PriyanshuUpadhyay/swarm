# Visual QA

The frame checks, the motion checks, the QA log, and the "visually verified" rule are in `blocks/visual-qa/SKILL.md`. This file holds build's own evidence and checks.

## Contents

- Required evidence
- Reference match
- Story checks
- Frame checks
- Motion checks
- Automated checks
- QA log

## Required evidence

Save representative full-resolution frames with stable names: the key frame of every beat, the
midpoint of every transition, and for a loop the frames on both sides of the seam.

```text
beat-01-<name>.png
trans-01-02-mid.png
seam-before.png
seam-after.png
contact-sheet.png
```


## Reference match

Score each style frame or key frame against its reference crop as `blocks/look-check/SKILL.md` says, with the compare image that
`blocks/headless-shot/SKILL.md` makes. In build, a `miss` blocks production scenes.

Example. Round 1 of a cloud frame scores the palette `match`, the tone `miss` (flat fills, no dots),
and the contour `partial` (right colour, even width). Round 2 adds the mass stack and line swell, and
both become `match`.

## Story checks

The key frame and pacing checks are in `blocks/story/SKILL.md`.

- Each transition has a story reason and the reason shows (a match cut really matches).
- Entities stay consistent across shots: same proportions, colours from the same tokens.
- Check each gate against the Gates section of [story-and-motion.md](story-and-motion.md). Play it once as a reader who fails.
- For a scroll story, check the settled-state rule of `blocks/web-ui-basics/SKILL.md`, and
  test step scrolling with the keyboard.

## Frame checks

In addition to the frame checks of the visual-qa block, inspect each decisive frame for:

- the mass, contour, colour, paper, and grain rules in `blocks/build-canvas/references/mark-making.md`, at 100 % zoom.

## Motion checks

In addition to the motion checks of the visual-qa block, inspect playback for:

- each continuity cut against Seam law in [story-and-motion.md](story-and-motion.md);
- boil against Motion feel in [story-and-motion.md](story-and-motion.md);
- a visible loop seam.

## Automated checks

Run these when frames can be rendered by `?t=`; they catch what a few stills miss:

- **Seam (loops):** render `t=0` and `t=1` and compare, for example
  `magick compare -metric RMSE a.png b.png null:`. Near zero (about 1e-4) means an exact return.
  `compare` exits 1 on any difference, so do not let `set -e` stop a script on it.
- **Pops:** compare every frame with the next. A step much larger than its neighbours (for example
  more than 1.25 times their mean) marks a jump, a reseed, or a skipped layer. Include the seam step.
  Run it with boil off (the `?boil=0` debug toggle). With boil on, it flagged every boil frame in a
  2026-09-30 trial.
- **Boil rate:** in a separate run with boil on, the large steps must come once per boil period, at
  the rate in Motion feel of [story-and-motion.md](story-and-motion.md).
- **Seam vectors:** for each continuity cut, read the carrier's screen velocity from `stateAt` on
  the last frame of shot A and the first frame of shot B. Flag a flipped sign on any axis, or a
  speed ratio outside about 0.5-2 (a starting threshold to tune). Skip cuts marked "jump".
- **Hidden content:** check it as `blocks/web-ui-basics/SKILL.md` says, with reduced motion on.
- **Read-back:** `ffprobe` the exported file for size, fps, frame count, and duration, and extract a
  few frames from it to inspect.

## QA log

The QA log goes in the step file that it checks: `04-style-frames`, `06-detail`, or `07-export`.
