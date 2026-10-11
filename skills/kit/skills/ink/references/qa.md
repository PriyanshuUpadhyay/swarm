# Visual QA

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

A contact sheet helps compare rhythm but does not replace inspection of full-size frames.

## Reference match

Put each style frame or key frame beside the matching reference crop, at the same scale:

```sh
magick ref-crop.png frame-crop.png -resize x900 +append compare-01.png
```

Score every line of the style grammar as `match`, `partial`, or `miss`, each with the pixel
evidence. Compare one crop of the whole frame and one crop at 100 % zoom, because texture and line
swell show only at full size. A `miss` on any trait blocks production scenes. Do at least two
rounds of fix and re-render. Record the scores in the step file. After round 3 with the same
`miss`, stop the step and show the user the frame and the miss. Keep
`accepted miss: <trait>, user: <their words>` in the step file, written only after the user has seen
the frame. Later rounds keep scoring every trait and exempt only that accepted miss from the
gate. A miss that changes asks again.

Example. Round 1 of a cloud frame scores the palette `match`, the tone `miss` (flat fills, no dots),
and the contour `partial` (right colour, even width). Round 2 adds the mass stack and line swell, and
both become `match`.

## Story checks

- Each beat's key frame reads on its own: the focal subject and the change of state are clear.
- Each transition has a story reason and the reason shows (a match cut really matches).
- Pacing lets each beat and each line of text read at playback speed, not only when paused.
- Entities stay consistent across shots: same proportions, colours from the same tokens.
- Each gate shows a hint for its action, answers a failed try, and has an escape. Play it once as a
  reader who fails.
- For a scroll story, every key fact is visible in a settled state and with
  `prefers-reduced-motion: reduce`. Test step scrolling with the keyboard.

## Frame checks

For each decisive frame inspect:

- focal point and eye path;
- crop, safe area, and accidental edge tangencies;
- contour consistency and shape readability;
- palette roles, contrast, and muddy overlaps;
- mask edges, transform discontinuities, and layer order;
- texture scale, density, flicker, and whether texture hides form;
- no flat mass near the camera, no contour of one even width, and no pure black unless the
  reference has it;
- paper and grain visible at 100 % zoom;
- lettering legibility, including what moving elements cover at different times. A moving shape
  that hides one letter can make a different word (a glint over the "C" turned "LOOK CLOSER" into
  "LOOK LOSER");
- originality changes relative to the references.

## Motion checks

Inspect playback at intended speed and at reduced speed:

- camera easing and speed spikes;
- parallax and nested-transform continuity;
- local motion phase relationships, and motion that restarts at a cut when it should continue;
- each continuity cut against the seam law: same axis, same direction, similar speed, cut
  mid-motion;
- a white flash or a see-through frame in the middle of a blend;
- unintended jitter caused by frame-rate dependence;
- texture crawling or random reseeding;
- boil at 6-12 changes per second, and no boil on text, halftone, or interface layers;
- visible loop seam;
- repeated motion that feels mechanically synchronized.

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
  the rate in the motion checks above.
- **Seam vectors:** for each continuity cut, read the carrier's screen velocity from `stateAt` on
  the last frame of shot A and the first frame of shot B. Flag a flipped sign on any axis, or a
  speed ratio outside about 0.5-2 (a starting threshold to tune). Skip cuts marked "jump".
- **Hidden content:** content that waits at opacity 0 for an animation is skipped by accessibility
  checkers such as axe-core and is blank in full-page screenshots. Check with reduced motion on.
- **Read-back:** `ffprobe` the exported file for size, fps, frame count, and duration, and extract a
  few frames from it to inspect.

## QA log

Keep the QA log as a table in the step file that it checks (`04-style-frames`, `06-detail`, or
`07-export`).

```text
Artifact/version:
Reference set:
Render settings:

Frame or interval | Observed pixel/playback evidence | Severity | Fix | Rechecked
```

Do not say "visually verified" unless pixels or playback from the final delivered artifact were
inspected after the last fix.
