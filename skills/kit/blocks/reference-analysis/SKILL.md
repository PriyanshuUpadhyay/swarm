---
name: reference-analysis
description: "Find, inspect, and measure every reference in five passes, with an evidence label on each fact, so the look and story rest on pixels. Use when work starts from visual references."
license: MIT
metadata:
  kind: step
  owns: "what a reference shows, measured, labelled, and what may be taken from it"
  serves: "design"
  needs: "a brief and its references, or a brief alone"
  gives: "measured notes per reference with evidence labels"
  conflicts: ""
  users: "ink, design-tokens"
  source: "skills/ink/references/references.md (research references, inspection, passes 1-5, evidence language; moved 2026-10-13); skills/ink/SKILL.md (contract: inspected pixels, borrow not copy, density)"
---

## Rules
Find the references
- Match a look only from pixels you inspected yourself. When the brief gives no references, find them from the brief before any design. Each part of the story and each quality of the look is a search.
- Use the `research` skill, and the `web-search` skill when one session cannot reach enough sites.
- Prefer primary pages (the artist's site, the studio, the museum, the film's frames) over reposts. Keep 4-8 works whose images or frames you can open and inspect.
- Record each work's source and what it informs (look, motion, structure, or more than one), and name one or two as the benchmark, each with what to learn from it and what not to take. A reference chosen for its story structure does not set the palette, and a still chosen for its palette does not set the pacing.
- Show the chosen set to the user, with one line on why each fits the brief.
Inspect
- Watch a whole video, and extract frames at scene boundaries, motion extremes, and loop seams. Inspect stills at full resolution and in useful crops.
- Analyse every reference in the five passes below. Tie each observation to a frame, a timestamp, or a crop.
- Pass 1, composition and hierarchy: aspect ratio, focal path, positive and negative space, horizon or dominant axes, framing, overlap, depth order, visual weight, and safe areas. Note how the eye enters and travels through the frame.
- Pass 2, shape and surface: silhouette families, geometric versus organic balance, contour weight, corner behavior, repeated motifs, fill behavior, texture scale, edge wear, material cues, and controlled imperfections. Distinguish drawn texture from image overlays when possible.
- Pass 2 measures, not only describes. Crop a typical mass at the delivery size and record the light direction and the layers on the mass (base, lit plane, core shadow, tone, rim, contour, material marks); the contour width range in px, where it is heaviest, and its colour role; how tone is made (halftone cell and angle, hatch spacing, dither, or a gradient) and where it appears; the marks per area, the grain scale, the paper colour, and any misregistration offset in px; the number of depth layers and how far layers change.
- Pass 3, color and type: give colors roles, not samples: background, dominant fill, secondary fill, highlight, shadow, contour, accent. Record saturation and value relationships. For lettering, describe construction, spacing, outline, deformation, and motion without copying protected wording or letterforms.
- Pass 4, motion and causality: classify each change as camera motion, object motion, deformation, reveal or mask, edit, or compositing. Record direction, amplitude, easing, duration, phase relationship, and loop behavior. Identify which parts stay rigid. Distinguish continuous spatial travel from cuts disguised by matching frames.
- Pass 5, story and structure, for a narrative reference: record the beats and where the story turns, shot sizes and their order, the length of each shot, the transition at each boundary and what it means, recurring motifs, and how on-screen text is timed. Map the result to a structure in the `story` block. Keep the structure; write your own content.
- Match density, not only palette. A number measured from a reference becomes a rule in the look, and the build meets it.
Evidence
- Label each fact `visible` (directly observed in pixels or frames), `stated` (claimed by the creator or supplied text), `inferred` (best explanation of observed behavior), or `unknown` (not shown or not recoverable).
- A `DESIGN.md` pulled from a live site or a collection is `stated` evidence. Measure the rendered pages before trusting its values.
- Do not upgrade `stated` or `inferred` to `visible`. A creator's claim about their process is evidence of the claim, not of the hidden workflow.
What may be taken
- Borrow techniques and high-level traits. Never copy characters, logos, signatures, exact compositions, or a living artist's exact personal style.

## Example
Initial state: the brief is "a small octopus mail sorter saves one letter from a flooded night post office", with no references.
Action: search flooded-city illustration, octopus character design, night post-office interiors, and print or comic looks. Keep six works with open images and name one comic page as the benchmark for line and tone, not for its characters. Crop a mass at delivery size: contour 2-5 px, heaviest on the shadow side, halftone cell 6 px at 45°, `visible`.
Result: the notes give each reference its use and each fact its label, and the halftone numbers become rules in the look.

## Check (script)
- none: the measured notes live in the step file, which a script check does not read.

## Check (judge)
- Does each reference have a source, what it informs, and an evidence label on each fact, with one or two named as the benchmark? Quote the list and the labels.
- Is each pass 2 number tied to a crop at delivery size? Quote the number and the crop.
- Does any `visible` label rest on a creator's claim instead of pixels? Quote the label and its basis.
