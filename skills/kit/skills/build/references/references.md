# Intake and style grammar

## Contents

- Intake
- Style grammar template

## Intake

Get these inputs before you build:

- the story, as a logline or brief, with the beats, the characters, props, and places, and any
  words on screen;
- visual references with provenance, researched from the brief when the user gives none;
- a start, a middle, and an end frame for each shot, from the user or extracted from a reference;
- the form, such as a still, a loop, a linear short, a journey, an interactive or scroll-driven
  piece, a reel background, or a website (see `blocks/build-site/SKILL.md`);
- the aspect ratio, the duration, and the fps;
- content limits, such as the subject, the words, brand assets, and things that must or must not
  appear;
- the delivery target when known, such as one HTML file, a source folder, frames, WebM, or MP4.

Pixels carry a look that words cannot, such as a halftone or a line weight. That is why each shot
needs its three frames.

If the user gives no story ("make something cool"), propose a 3-5 beat story, one line per beat,
and build that. Do not default to a pattern you used before.

Fill low-risk gaps with defaults and record them. Use 1080x1920 for a vertical reel, 30 fps, 6-12 s
for a loop, and 15-45 s for a short. Do not guess exact brand copy, logos, likenesses, or publishing
destinations. Ask for them.

Record each reference in `ink/01-intake.md`. Find, inspect, and measure the references as `blocks/reference-analysis/SKILL.md` says. In build, the recipes in
`blocks/build-canvas/references/mark-making.md` give starting numbers where a reference is too small to measure.

## Style grammar template

```text
Reference set:
Benchmark (what to learn, what not to take):
Target feeling:
Visible facts:
Inferences:
Unknowns:

Palette roles:
Light direction:
Mass stack (layers per mass near the camera):
Contour rules (width range px, swell, heavy side, colour role):
Tone method (halftone cell and angle, hatch thresholds, where it appears):
Mark budget:
Shape vocabulary:
Composition/depth (number of layers, aerial perspective):
Texture systems (paper, grain, misregistration, each with its opacity or offset):
Type behavior:
Motion grammar:
Boil policy (rate, which layers boil):
Story structure:
Controlled irregularity:
Do not drift into:
Originality changes:
```

Write the grammar to `ink/02-look.md`, and turn it into one `look` token object. A scene that
needs a different look (a flashback, a dream) gets a named variant of the same token keys, not new
literals.
