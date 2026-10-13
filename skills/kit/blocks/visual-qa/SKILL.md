---
name: visual-qa
description: "Rendered frames and playback are inspected at full size against a list of frame and motion faults, and each finding goes in a QA log with its pixel evidence."
license: MIT
metadata:
  kind: step
  owns: "the visual verdict on rendered frames and playback, and the QA log"
  serves: "review"
  needs: "rendered frames or a playable build, and the look"
  gives: "a QA log with one row per finding"
  conflicts: ""
  users: "ink"
  source: "skills/ink/references/qa.md (required evidence, frame checks, motion checks, QA log; moved 2026-10-13); blocks/look-check/SKILL.md (visually verified)"
---

## Rules
Evidence
- Inspect full-resolution frames. A contact sheet helps compare rhythm but does not replace inspection of full-size frames.
- Inspect playback at the intended speed and at reduced speed.
Frame checks, for each decisive frame
- Focal point and eye path.
- Crop, safe area, and accidental edge tangencies.
- Contour consistency and shape readability.
- Palette roles, contrast, and muddy overlaps.
- Mask edges, transform discontinuities, and layer order.
- Texture scale, density, flicker, and whether texture hides form.
- Lettering legibility, including what moving elements cover at different times. A moving shape that hides one letter can make a different word (a glint over the "C" turned "LOOK CLOSER" into "LOOK LOSER").
- Originality changes relative to the references.
Motion checks
- Camera easing and speed spikes.
- Parallax and nested-transform continuity.
- Local motion phase relationships, and motion that restarts at a cut when it should continue.
- A white flash or a see-through frame in the middle of a blend.
- Unintended jitter caused by frame-rate dependence.
- Texture crawling or random reseeding.
- Repeated motion that feels mechanically synchronized.
QA log
- Keep the QA log as a table in the step's own file:
  `Artifact/version:`, `Reference set:`, `Render settings:`, then rows of `Frame or interval | Observed pixel/playback evidence | Severity | Fix | Rechecked`.
- Do not say "visually verified" unless pixels or playback from the final delivered artifact were inspected after the last fix.

## Example
Initial state: a reel's export is done and its beat key frames are saved full size.
Action: inspect each key frame at 100 % and play the export at full and half speed. The frame at 3.2 s shows a white flash in the storm-to-dawn blend, and the glint covers the "C" of "LOOK CLOSER".
Result: the QA log has two rows with the frame times, the fixes (clear to the ground colour before each draw, move the glint path), and `Rechecked` filled only after both frames were inspected again in the new export.

## Check (script)
- none: the QA log lives in the step file, which a script check does not read.

## Check (judge)
- Does the QA log name the artifact version and have one row per finding, each with its pixel or playback evidence? Quote the rows.
- Was each fixed row rechecked in the final delivered artifact? Quote the rechecked frames.
- Does the step say "visually verified" anywhere without an inspection of the final artifact after the last fix? Quote it.
