---
name: build-canvas
description: "Build a drawn piece as a small story engine of tokens, entities, scenes, a shot list, and transitions, rendered from time and one seed. Use when the piece is a canvas, p5, or three.js reel or plate."
license: MIT
metadata:
  kind: pack
  owns: "how a canvas, p5.js, WebGL, or WebGPU piece is built so every frame renders from time and one seed"
  serves: "build"
  needs: "the look tokens, the story sheet, and the shot list"
  gives: "a piece that renders any frame from ?t= and its seed"
  conflicts: ""
  users: "build"
  source: "skills/ink/references/build.md (architecture, determinism, debug modes, performance, toolbox, delivery, technical references) and skills/ink/references/mark-making.md, moved 2026-10-13"
---

## Rules
- Run from the repo root when you use this block's script checks.
The item file `blocks/build-canvas/references/mark-making.md` in the kit holds the mark-making recipes: density, tone, texture, the mass stack, line, surface, depth, generative plates, and libraries. Read it before a detail pass.

Architecture
Build a small story engine, so the story, the look, and the motion change in one place each. Each entity of the story sheet becomes a draw function with parameters (pose, expression, state, seed), not a one-off drawing, so it appears in many shots and states without new code:

```text
config / seed
look          token object from the look file, the step file that holds the look (palette roles, light direction, line widths,
              halftone cell, texture opacities); the passes are in references/mark-making.md
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

Determinism
- Store one seed with the output.
- Reset p5 `randomSeed()` and `noiseSeed()` predictably. Seed decor per entity or scene type, not per
  frame.
- Never use wall-clock values for render state. Live playback only advances `t`.
- Accept `?t=` (or a frame number) so any single frame renders alone. Frame export and QA need this.
  A scroll or gate story has no single clock, so accept `?shot=<id>&u=` instead.
- Expose `stateAt(t)` (or `stateAt(shot, u)`): the camera and each carrier's screen position. The
  seam-vector check in build's `skills/build/references/qa.md` compares these values on both sides of a cut.
- A stateful simulation (particles with physics, reaction-diffusion, fluid) cannot jump to any `t`.
  Step it with a fixed time step from frame 0, export frames in order in one session, and keep
  checkpoints every N frames so `?t=` can resume from the nearest one.
- On the GPU, `seek(t)` resolves only after the frame is on the canvas, so a screenshot never
  catches a half frame. Render, then wait for `device.queue.onSubmittedWorkDone()` and two
  animation frames. Do not await `renderer.renderAsync()`, because three.js deprecated it in r181.
  A 2026-09-30 three.js WebGPU trial captured full frames this way.

Debug modes
Give toggles for:

- shot id, beat, shot-local `u`, and a timeline scrub;
- scene bounds, masks, and portals;
- camera centre and scale;
- safe areas for the delivery format;
- texture on and off, and layer isolation;
- boil on and off (`?boil=0`);
- a reference overlay when rights and source allow it.

Performance
Prototype at reduced resolution. Cache static textures. Do not redraw costly grain or thousands of
hatch marks every frame unless they move. Profile before cutting visual detail. Export from a fixed
timestep so slow rendering does not change the motion.

Toolbox
Choose tools for the look and the story, not for a small build. Dependencies, a loading screen, 3D
models, shaders, and WebGPU are all acceptable when they make the piece better. Every tool still
renders from `seek(t)` and the seed.

Example. A comic reel with a camera that flies through a 3D city: three.js draws the city with
stepped toon light, a post pass draws ink edges from depth and normals, a second pass puts halftone
dots only in the shadow band, and a paper pass multiplies over the frame. This is the mass stack from
[references/mark-making.md](references/mark-making.md) on a GPU.

| Need | Tool | Note |
|---|---|---|
| 2D illustration, print looks | Canvas 2D or p5.js 2.x | recipes in references/mark-making.md |
| Pencil, marker, hatching, watercolor fills | p5.brush (MIT) | p5.js WEBGL mode or WebGL2 only |
| 3D scenes, cameras, glTF models | three.js (MIT), `WebGPURenderer` with its WebGL2 fallback | TSL node materials run on both back ends |
| Shader post passes (ink edges, halftone, dither, paint filter, grain, misregistration) | three.js TSL post nodes, or pmndrs `postprocessing` (Zlib) on WebGL | one pass per surface layer of the mass stack |
| Simulation at scale (millions of particles, reaction-diffusion, fluid, flocking) | WebGPU compute (three.js TSL compute or WGSL) | stateful, see Determinism |
| Shader function library (noise, dither, filters, SDFs) | Lygia | Prosperity licence, free only for non-commercial work |
| Timeline, easing, text choreography | GSAP (free no-charge licence, not open source) | drive a paused timeline with `tl.seek(t)` |
| Scroll-driven story | GSAP ScrollTrigger or CSS scroll timelines | see the scroll driver in build's `skills/build/references/story-and-motion.md` |
| Sound | Web Audio | cues from the shot list |

Procedural 3D can read as a game, not as an illustration. In a 2026-09-30 trial, a three.js scene
built from plain boxes read as a cel-shaded game. A 2D Canvas build of the same reference read as
illustration. For an illustrated look, draw the masses in 2D and place them as cards in 3D space
(2.5D parallax). Or give 3D shapes irregular silhouettes, bevels, and dense set dressing (props
that fill the scene). Use full 3D when the camera move is the story.

Measure first-frame load time and show a loading state that fits the look. Load time is a trade-off,
not a limit.

Delivery
- Style frame or small piece: one HTML file, with libraries from a CDN through an import map.
- Larger story: a Vite project with `look.js`, `scenes/`, `story.js` (the shot list), `main.js`, and a
  README. Browsers block ES modules over `file://`, so serve it by the `serve-local` block with its dev or preview command.
- Offline delivery: vendor dependencies explicitly, in a `vendor/` folder.

Technical references
- p5.js reference: https://p5js.org/reference/
- p5.js `createGraphics`: https://p5js.org/reference/p5/createGraphics/
- p5.js `noise`: https://p5js.org/reference/p5/noise/
- p5.js `saveCanvas`: https://p5js.org/reference/p5/saveCanvas/
- Canvas 2D transforms: https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/setTransform
- Browser animation timing: https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame
- CCapture.js for fixed-timestep browser capture: https://github.com/spite/ccapture.js/

Availability and browser support can change. Check the current official pages before choosing an
export route for a live project.

## Example
Initial state: a reel of five beats has the shot list, the `look` tokens, and one scene module per place.
Action: the user asks for the storm to last two seconds longer. Edit only the `storm` entry's `dur` in the shot list, then render `?t=` at the storm key frame and at both transition midpoints with the stored seed.
Result: the scene modules, the entities, and the tokens are unchanged, and each frame renders alone with the same pixels on a second run.

## Check (script)
- `rg -n -g '*.html' -g '*.js' -g '*.mjs' -g '*.ts' -g '!**/vendor/**' -g '!**/node_modules/**' -g '!*.min.js' 'Math\.random\(|Date\.now\(' {{site}}` prints nothing: no render state in the piece's own code comes from an unseeded random or the wall clock.

## Check (judge)
- Does one change of each kind (a retime, a new place, a new character, a restyle, a new transition) touch only its one home? Quote the files each touched.
- Do two renders of the same `?t=` with the stored seed give the same frame? Quote both frame paths and the compare result.
- Does a blend frame show no white flash or see-through frame, because the frame is cleared to the look's ground colour first? Quote the frame path.
