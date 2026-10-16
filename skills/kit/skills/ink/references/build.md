# Build

The story engine, determinism, debug modes, performance, the toolbox, and delivery are in `blocks/build-canvas/SKILL.md`. This file holds ink's own build steps.

## Contents

- Style frames
- Animatic
- Frame export
- Handoff

## Style frames

Style frames are step 4 in the skill. A design canvas can hold the board of style frames:
[pen.dev](https://www.pen.dev/) (formerly Pencil) has an MCP server and a `pen` CLI and exports PNG
and HTML, but it needs an account and has no timeline, so it cannot preview motion.

## Animatic

Build the shot list, the scene modules as flat shapes, the transitions, the timing, and the loop
seam if there is one. Render the required evidence frames in [qa.md](qa.md). Fix readability,
pacing, crop, and continuity before you add any detail.

## Frame export

Render each frame at the delivery size, such as 1080,1920 for a vertical reel, at `?t=0.25`
(or `?shot=<id>&u=`). Use the capture commands and backend choice in
`blocks/headless-shot/SKILL.md`; its page widths and page names do not apply to reel frames.
The GPU readiness contract stays in `blocks/build-canvas/SKILL.md`. Run independent captures at
once, then encode with ffmpeg (`-c:v libx264 -pix_fmt yuv420p`).

For a DOM, CSS, or GSAP composition instead of a canvas,
[HyperFrames](https://github.com/heygen-com/hyperframes) renders HTML to video in headless Chrome
and ships its own agent skills.

## Handoff

Keep the coded artwork apart from editorial layers (presenter cutout, captions, titles, sound).
Merge them only when the user asks for one build.

Deliver these items:

- the source and the exact run command;
- the seed and the parameter preset;
- still and export controls, and frame capture instructions;
- the duration, fps, dimensions, and loop and alpha behaviour;
- editor notes with safe areas;
- the step files in `ink/`, which hold the notes, the story, the scores, and the QA log.
