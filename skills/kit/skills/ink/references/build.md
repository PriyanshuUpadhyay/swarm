# Build

## Contents

- Architecture
- Determinism
- Debug modes
- Performance
- Toolbox
- Delivery
- Style frames
- Animatic
- Frame export
- Handoff
- Technical references

## Architecture

Build a small story engine, so the story, the look, and the motion change in one place each:

```text
config / seed
look          token object from ink/02-look.md (palette roles, light direction, line widths,
              halftone cell, texture opacities); the passes are in mark-making.md
entities      draw functions with parameters: character(ctx, look, {pose, mood, seed})
scenes        draw(ctx, u, t, look, params): own coordinates, no colour literals, no story timing
shots         data: scene id, duration, camera keys, params, transition in
transitions   (ctx, drawA, drawB, k, params): how two shots meet
cameraAt      (shot, u) -> transform
render(t)     active shot or shots -> camera -> scene (flat tints) -> transition -> tone pass
              -> line pass -> surface pass (paper, grain, plates) -> captions -> debug
export hooks
```

A minimal render loop:

```js
function render(t) {
  const { shot, u, incoming, uIn, k } = shotAt(t);          // from the shot list only
  const draw = (s, uu) => () => withCamera(s, uu, () => SCENES[s.scene](ctx, uu, t, look, s.params));
  if (incoming) TRANSITIONS[incoming.in.type](ctx, draw(shot, u), draw(incoming, uIn), k, incoming.in);
  else draw(shot, u)();
  texturePass(); captions(t); if (debug) overlay(shot, u);
}
```

Each kind of change has one home:

| Change | Edit |
|---|---|
| new beat or retiming | shot list |
| new place | one scene module |
| new character or prop | one entity function |
| restyle | look tokens (same keys, new values), or a named variant |
| new transition | one entry in the transition map |

If one change needs edits in two of these homes, a boundary is wrong. Fix the boundary first.

Clear the frame with the look's ground colour before every draw. During a blend the two shots'
opacity can sum below 1, and whatever sits behind them (a white page, the last frame) flashes
through.

Use `push()` / `pop()` in p5.js or `save()` / `restore()` in Canvas around every nested transform or
clip, so no transform leaks. Use offscreen buffers or cached `Path2D` objects for masks, grain,
costly textures, and reused nested scenes.

## Determinism

- Store one seed with the output.
- Reset p5 `randomSeed()` and `noiseSeed()` predictably. Seed decor per entity or scene type, not per
  frame.
- Never use wall-clock values for render state. Live playback only advances `t`.
- Accept `?t=` (or a frame number) so any single frame renders alone. Frame export and QA need this.
  A scroll or gate story has no single clock, so accept `?shot=<id>&u=` instead.
- Expose `stateAt(t)` (or `stateAt(shot, u)`): the camera and each carrier's screen position. The
  seam check in [qa.md](qa.md) compares these values on both sides of a cut.
- A stateful simulation (particles with physics, reaction-diffusion, fluid) cannot jump to any `t`.
  Step it with a fixed time step from frame 0, export frames in order in one session, and keep
  checkpoints every N frames so `?t=` can resume from the nearest one.
- On the GPU, `seek(t)` resolves only after the frame is on the canvas, so a screenshot never
  catches a half frame. Render, then wait for `device.queue.onSubmittedWorkDone()` and two
  animation frames. Do not await `renderer.renderAsync()`, because three.js deprecated it in r181.
  A 2026-09-30 three.js WebGPU trial captured full frames this way.

## Debug modes

Give toggles for:

- shot id, beat, shot-local `u`, and a timeline scrub;
- scene bounds, masks, and portals;
- camera centre and scale;
- safe areas for the delivery format;
- texture on and off, and layer isolation;
- boil on and off (`?boil=0`);
- a reference overlay when rights and source allow it.

## Performance

Prototype at reduced resolution. Cache static textures. Do not redraw costly grain or thousands of
hatch marks every frame unless they move. Profile before cutting visual detail. Export from a fixed
timestep so slow rendering does not change the motion.

## Toolbox

Choose tools for the look and the story, not for a small build. Dependencies, a loading screen, 3D
models, shaders, and WebGPU are all acceptable when they make the piece better. Every tool still
renders from `seek(t)` and the seed.

Example. A comic reel with a camera that flies through a 3D city: three.js draws the city with
stepped toon light, a post pass draws ink edges from depth and normals, a second pass puts halftone
dots only in the shadow band, and a paper pass multiplies over the frame. This is the mass stack from
[mark-making.md](mark-making.md) on a GPU.

| Need | Tool | Note |
|---|---|---|
| 2D illustration, print looks | Canvas 2D or p5.js 2.x | recipes in mark-making.md |
| Pencil, marker, hatching, watercolor fills | p5.brush (MIT) | p5.js WEBGL mode or WebGL2 only |
| 3D scenes, cameras, glTF models | three.js (MIT), `WebGPURenderer` with its WebGL2 fallback | TSL node materials run on both back ends |
| Shader post passes (ink edges, halftone, dither, paint filter, grain, misregistration) | three.js TSL post nodes, or pmndrs `postprocessing` (Zlib) on WebGL | one pass per surface layer of the mass stack |
| Simulation at scale (millions of particles, reaction-diffusion, fluid, flocking) | WebGPU compute (three.js TSL compute or WGSL) | stateful, see Determinism |
| Shader function library (noise, dither, filters, SDFs) | Lygia | Prosperity licence, free only for non-commercial work |
| Timeline, easing, text choreography | GSAP (free no-charge licence, not open source) | drive a paused timeline with `tl.seek(t)` |
| Scroll-driven story | GSAP ScrollTrigger or CSS scroll timelines | see the scroll driver in [story-and-motion.md](story-and-motion.md) |
| Sound | Web Audio | cues from the shot list |

Procedural 3D can read as a game, not as an illustration. In a 2026-09-30 trial, a three.js scene
built from plain boxes read as a cel-shaded game. A 2D Canvas build of the same reference read as
illustration. For an illustrated look, draw the masses in 2D and place them as cards in 3D space
(2.5D parallax). Or give 3D shapes irregular silhouettes, bevels, and dense set dressing (props
that fill the scene). Use full 3D when the camera move is the story.

Measure first-frame load time and show a loading state that fits the look. Load time is a trade-off,
not a limit.

## Delivery

- Style frame or small piece: one HTML file, with libraries from a CDN through an import map.
- Larger story: a folder with `look.js`, `scenes/`, `story.js` (the shot list), `main.js`, and a
  README, or a Vite project. Browsers block ES modules over `file://`, so give a local server command.
- Offline delivery: vendor dependencies explicitly.

## Style frames

Style frames are step 4 in the skill. A design canvas can hold the board of style frames:
[pen.dev](https://www.pen.dev/) (formerly Pencil) has an MCP server and a `pen` CLI and exports PNG
and HTML, but it needs an account and has no timeline, so it cannot preview motion.

## Animatic

Build the shot list, the scene modules as flat shapes, the transitions, the timing, and the loop
seam if there is one. Render the required evidence frames in [qa.md](qa.md). Fix readability,
pacing, crop, and continuity before you add any detail.

## Frame export

Render frames with a headless Chromium: `chrome-headless-shell --headless --window-size=W,H
--screenshot=f.png "file://.../index.html?t=0.25"`. Playwright caches one under
`~/Library/Caches/ms-playwright/`. Run several at once, then encode with ffmpeg
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

## Technical references

- p5.js reference: https://p5js.org/reference/
- p5.js `createGraphics`: https://p5js.org/reference/p5/createGraphics/
- p5.js `noise`: https://p5js.org/reference/p5/noise/
- p5.js `saveCanvas`: https://p5js.org/reference/p5/saveCanvas/
- Canvas 2D transforms: https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/setTransform
- Browser animation timing: https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame
- CCapture.js for fixed-timestep browser capture: https://github.com/spite/ccapture.js/

Availability and browser support can change. Check the current official pages before choosing an
export route for a live project.
