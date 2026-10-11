# Intake and reference analysis

## Contents

- Intake
- Research references
- Pass 1: composition and hierarchy
- Pass 2: shape and surface
- Pass 3: color and type
- Pass 4: motion and causality
- Pass 5: story and structure (narrative references)
- Style grammar template
- Evidence language

## Intake

Get these inputs before you build:

- the story, as a logline or brief, with the beats, the characters, props, and places, and any
  words on screen;
- visual references with provenance, researched from the brief when the user gives none (see
  Research references below);
- one or two references named as the benchmark, each with what to learn from it and what not to
  take;
- a start, a middle, and an end frame for each shot, from the user or extracted from a reference;
- the form, such as a still, a loop, a linear short, a journey, an interactive or scroll-driven
  piece, a reel background, or a website (see [site.md](site.md));
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

Inspect every reference yourself. Watch a whole video, and extract frames at scene boundaries,
motion extremes, and loop seams. Inspect stills at full resolution and in useful crops. Record each
reference in `ink/01-intake.md` with what it informs (look, motion, structure, or more than one).
A reference chosen for its story structure does not set the palette, and a still chosen for its
palette does not set the pacing.

Then analyse every reference in the passes below. Tie each observation to a frame, a timestamp, or
a crop.

## Research references

When the brief gives no references, find them from the brief before any design. Each part of the
story and each quality of the look is a search.

Example. The brief is "a small octopus mail sorter saves one letter from a flooded night post
office". Search for flooded-city illustration, octopus character design, night post-office
interiors, and print or comic looks that fit a small brave hero. Keep 4-8 works whose images or
frames you can open and inspect.

- Use the `research` skill, and the `web-search` skill when one session cannot reach enough sites.
- Prefer primary pages (the artist's site, the studio, the museum, the film's frames) over reposts.
- Record each work's source and what it informs, and name one or two as the benchmark.
- Show the chosen set to the user at intake, with one line on why each fits the brief.

## Pass 1: composition and hierarchy

Record aspect ratio, focal path, positive and negative space, horizon or dominant axes, framing,
overlap, depth order, visual weight, and safe areas. Note how the eye enters and travels through the
frame.

## Pass 2: shape and surface

Record silhouette families, geometric versus organic balance, contour weight, corner behavior,
repeated motifs, fill behavior, texture scale, edge wear, material cues, and controlled
imperfections. Distinguish drawn texture from image overlays when possible.

Measure, do not only describe. Crop a typical mass at the delivery size and record:

- the light direction, and the layers on the mass (base, lit plane, core shadow, tone, rim,
  contour, material marks);
- the contour width range in px, where it is heaviest, and its colour role;
- how tone is made (halftone cell and angle, hatch spacing, dither, or a gradient) and where it
  appears;
- the marks per area, the grain scale, the paper colour, and any misregistration offset in px;
- the number of depth layers and how far layers change.

A number measured from the reference becomes a rule in the style grammar. The recipes in
[mark-making.md](mark-making.md) give starting numbers where a reference is too small to measure.

## Pass 3: color and type

Assign palette roles rather than sampling isolated colors: background, dominant fill, secondary fill,
highlight, shadow, contour, accent. Record saturation and value relationships. For lettering,
describe construction, spacing, outline, deformation, and motion without copying protected wording
or letterforms.

## Pass 4: motion and causality

For video, classify each change as camera motion, object motion, deformation, reveal/mask, edit, or
compositing. Record direction, amplitude, easing, duration, phase relationship, and loop behavior.
Identify which parts stay rigid. Distinguish continuous spatial travel from cuts disguised by
matching frames.

## Pass 5: story and structure (narrative references)

Record the beats and where the story turns, shot sizes and their order, the length of each shot,
the transition at each boundary and what it means, recurring motifs, and how on-screen text is
timed. Map the result to a structure in [story-and-motion.md](story-and-motion.md) (linear, loop,
journey, vignette, branching). Keep the structure; write your own content.

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

## Evidence language

Use:

- `visible`: directly observed in pixels or frames;
- `stated`: claimed by the creator or supplied text;
- `inferred`: best explanation of observed behavior;
- `unknown`: not shown or not recoverable.

Do not upgrade `stated` or `inferred` to `visible`. A creator's claim about their process is
evidence of the claim, not of the hidden workflow.
