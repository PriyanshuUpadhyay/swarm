# Build

The story engine, determinism, debug modes, performance, the toolbox, and delivery are in `~/.claude/blocks/build-canvas/SKILL.md`. This file holds ink's own build steps.

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

Render each frame at the delivery size, such as 1080,1920 for a vertical reel:
`<chrome-headless-shell> --headless --window-size=W,H --screenshot=f.png "file://.../index.html?t=0.25"`
(or `?shot=<id>&u=`). Find `<chrome-headless-shell>` as `~/.claude/blocks/headless-shot/SKILL.md`
says; its page widths and shot names are for page shots, not reel frames. Run several at
once, then encode with ffmpeg
(`-c:v libx264 -pix_fmt yuv420p`). Lightpanda has no paint engine, and Obscura 0.2.2 drew canvas
transforms, arcs, clips, and text wrong in a 2026-09 test, so neither can render canvas frames.

For WebGPU, use the full Chrome for Testing binary from Playwright's `chromium-*` folder with
`--headless=new --enable-unsafe-webgpu --use-mock-keychain`. In a 2026-09 test on macOS arm64,
`chrome-headless-shell` drew a blank WebGPU canvas and Chrome for Testing drew it correctly. Check
one frame before a long export.

On macOS, headless Chrome for Testing reads its "Safe Storage" key from the login keychain at start,
so macOS shows a keychain prompt during frame capture. Always launch it with `--use-mock-keychain`
and a fresh `--user-data-dir`. If a prompt still appears, Deny is safe, because the capture profile
is empty and the frames still render.

The `--screenshot` flag cannot wait for an async GPU render. For WebGPU, drive Chrome for Testing
over the DevTools protocol. Call `window.seek(t)` with `Runtime.evaluate` and `awaitPromise: true`,
then call `Page.captureScreenshot`. The 2026-09-30 WebGPU trial exported its frames this way.

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
