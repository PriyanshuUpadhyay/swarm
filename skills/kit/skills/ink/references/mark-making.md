# Mark-making: density, tone, and texture

## Contents

- Order of work
- Detail passes
- The mass stack
- Line
- Tone from marks
- Surface
- Depth
- Generative plates
- Libraries
- Sources

A frame with the right palette but flat fills reads as clip art. Frames that people call
hand-drawn have more layers on each mass, lines that swell, tone made from marks, and a paper
surface. This file gives recipes with starting numbers. Measure the reference, then replace the
numbers with the measured ones in `ink/02-look.md`.

Example. A 2026-09 trial drew a cloud tunnel with one flat fill and one even black outline per ring.
The reference cloud had a base, a lit plane, a core shadow, halftone dots only in that shadow, a
navy contour that was heavier on the shadow side, small curl marks inside, and paper grain. That is
seven layers against two, and the trial read as clip art.

## Order of work

Draw flat, print later. Scene code draws flat shapes with tints, and later passes turn them into
marks:

1. **Scene.** Flat shapes from look tokens. A tint (alpha below 1) means tone, not transparency.
2. **Tone.** Turn tints into halftone dots or hatching, and keep full ink solid.
3. **Line.** Contours with swell, wobble, and a weight hierarchy.
4. **Surface.** Paper, grain, misregistration, and ink voids, multiplied over the frame.
5. **Editorial.** Captions and titles, kept out of the texture passes.

Cache passes 2 and 4 per state. Do not redraw thousands of dots on every frame when nothing moved.

Draw the style frames in this order with the real drawing code. When a character recurs, add a
model sheet that shows its poses and expressions.

## Detail passes

After the animatic, bring every scene up to the approved style frames. Add one system at a time,
always through the look tokens, in this order: mass stacks, line hierarchy, tone marks, depth
layers and masks, surface texture, secondary motion and boil, lettering, and final colour.

Re-render the same decisive frames after each pass, so a regression shows. Do not hide a mismatch
under texture.

## The mass stack

Nothing flat near the camera. Give each mass these layers, all lit from one light direction per shot:

| Layer | Build |
|---|---|
| Core shadow | fill the whole shape in the shade token |
| Tone | halftone or hatch over the shape, in a darker ink |
| Base | the same shape moved 8-15 % of its size toward the light, so a shadow crescent stays on the far side |
| Lit plane | a smaller shape near the lit edge in the light token, or a thin rim line |
| Contour | the line pass below, heavier on the side away from the light |
| Material marks | curls, seams, cracks, or fibres, clipped inside the silhouette |

```js
function mass(shape, box, L, look, seed) {       // shape: Path2D; L: unit vector toward the light
  const d = 0.15 * Math.min(box.w, box.h);
  ctx.save(); ctx.clip(shape);
  ctx.fillStyle = look.shade; ctx.fill(shape);
  ctx.save(); ctx.translate(L.x * d, L.y * d); ctx.fillStyle = look.base; ctx.fill(shape); ctx.restore();
  // dots after the base: tone 0 on the lit half, rising to about 0.6 at the far edge
  halftone(box, look.ink, { cell: 7, angle: 45, seed, tone: p => 0.6 * farSide(p, box, L) });
  ctx.restore();
}
```

The moved base leaves a shadow crescent on the far side. The dots go on after the base, so they fade
across it into the crescent, as in print. Keep the dot tone at about 0.6 or less. At a tone of 0.965
or more the dots merge into solid ink, and a thin crescent then reads as a flat band.

A scalloped cloud is a union of 6-12 lobes along a noisy ellipse. Make the lobes ellipses with an
aspect of 0.6-1 and a random turn, and flatten their undersides. Equal round circles read as bubble
wrap. To outline a union, stroke every lobe at twice the line width, then fill every lobe on top. Only
the outer edge of the stroke stays visible.

Add more marks inside a silhouette before you add more objects. Bark, end grain, soot, chipped stone,
and canvas seams make a frame rich at a low cost.

## Line

- **Colour.** Use the darkest palette role (navy, brown, or a darker tint of the fill). Use pure black
  only when the reference has it.
- **Swell.** Width along the stroke is `w * (0.2 + 0.8 * Math.sin(Math.PI * u) ** 0.7)`, tapered over
  the first and last 22 %. Never use one width for a whole contour. Build the stroke as a filled
  outline (`perfect-freehand` `getStroke`), or stroke short segments with a round cap.
- **Hierarchy.** Silhouette 1, inner edges about 0.5, marks about 0.25. A frame with only silhouette
  lines reads as vector art. Add thin interior strokes, some broken, and a few hatch or speed strokes
  that cross a mass. Near objects are about 2.5 times
  thicker than far ones. Under a camera zoom, scale the width by `zoom ** -0.55` so close-ups do not
  get fat lines.
- **Wobble.** Move each point along the line's normal by two summed sines with seeded phases.
  Sample every 6 px, with an amplitude of 0.6 px for clean ink and 1-2.5 px for a sketch.

```js
const wob = s => amp * (0.72 * Math.sin(s * f1 + p1) + 0.28 * Math.sin(s * f2 + p2)); // f1 .018-.038, f2 .07-.12
// point at arc length s: [x + nx * wob(s), y + ny * wob(s)]; phases p1, p2 come from rng(seed, boil)
```

- **Sketch looks.** Overshoot 3-8 px at corners, and leave one gap in a third to half of the long
  strokes.

## Tone from marks

- **Halftone.** Dots on a rotated grid, with the radius `Math.sqrt(tone) * 0.7 * cell`. At a factor of
  about 0.7 the dots merge into solid ink. A tint of 0.965 or more prints solid. The cell is about 3 px
  for a fine print and 20-26 px for a poster look. Give each ink its own screen angle (for example 15°,
  75°, 0°, 45°). Draw all dots of a layer as one path.
- **Hatching.** Hatch families switch on at tones 0.14, 0.5, and 0.75. The direction follows the
  form, and the width follows the tone. Clip the hatching to the shape, and keep it in the
  element's local space so it moves with the element.
- **Ordered dither.** Index a Bayer matrix by screen pixel, so the pattern stays fixed on screen.
- **"Halftone only in shadow"** is a tone threshold, not a choice for each shape.

Set a mark budget. More marks do not add quality, so put them where the form turns:

- Hatch only the shadow edge, with 3-6 cuts. Hatching on every surface reads as fur or rain.
- Use at most two plates in one area, and only one of them as a tint.
- Two tufts read as fur, and four read as torn paper.

## Surface

- **Paper.** Draw one seeded tile once, cache it, and repeat it as a pattern multiplied over
  everything. One working tile is 220 px with 1,300 specks at alpha 0.05 or less and 26 curved fibres
  at alpha 0.09. Add low-frequency mottling (a small random tile scaled up) and a soft vignette. SVG
  `feTurbulence` (`baseFrequency` 0.9, `numOctaves` 2, `stitchTiles`) also works as a tile.
- **Grain.** A few hundred specks of paper colour and fewer ink specks. Re-roll them every 2 frames, or
  re-roll a third of them. A full re-roll on every frame flickers. Draw glows under the grain.
- **Print plates.** Give each ink an offscreen canvas. Draw only alpha into it, colour it with
  `source-in`, and composite with `multiply` at alpha about 0.93. Clear the other plates under a shape
  before it adds its own ink (knockout). Cut ink voids with `destination-out` specks.
- **Misregistration.** Give each plate a fixed offset per shot, below 1 px for a fine print and 2-4 px
  for a loud zine look. Move it only on an accent. A random offset on every frame shakes.
- **Pigment mixing.** For overlapping washes, `spectral.mix` (Kubelka-Munk) gives paint-like results
  where RGB averages go grey.

## Depth

Use 5-8 depth layers. Far layers take on the colour of the air, lose contrast and line weight, and
carry fewer marks. Put a strip of mist between layers. One maker of a code-drawn film said, "What made the frames rich was mostly layers".

## Generative plates

A plate is not done until it has a surface: paper, grain, and one imperfection (an ink void, a
misregistered plate, or a stray mark).

| Plate | Core | Starting numbers |
|---|---|---|
| Halftone sphere | Lambert light field drives the dot radius | light upper left, cell 6-26 px, factor 0.7 |
| Stipple or soft texture | many small marks at low alpha with Gaussian spread | 20,000 dots of 2×2 px, or 8,000 lines in clusters of 8; 100 layers at alpha 0.02 |
| Marbling (domain warp) | `f = fbm(p + 4r)`, `r = fbm(p + 4q + ...)`, `q = fbm(p + ...)` | offsets (5.2,1.3), (1.7,9.2), (8.3,2.8), factor 4.0; bands from `fract(f * n)` |
| Flow field | curves step through a noise angle grid | grid 0.5 % of width, noise step 0.005, step 0.1-0.5 % of width, 500-1000 steps for fur; angles quantized to π/4 for rock; start from circle packing, stop at a collision |
| Reaction-diffusion | Gray-Scott on a grid | DA 1.0, DB 0.5, f 0.055, k 0.062, Δt 1; Laplacian centre -1, sides 0.2, corners 0.05 |
| Clifford attractor | `x' = sin(a y) + c cos(a x)`, `y' = sin(b x) + d cos(b y)` | a -1.4, b 1.6, c 1.0, d 0.7; count hits per 3 px cell, then place dots by density divided by its 98.5th percentile, gamma about 0.85, up to 2 dots per cell (a log-density map gave flat noise in a trial) |
| Watercolor | polygon deformed by midpoint Gaussian displacement | 7 passes for the base, 4-5 more per layer; 30-100 layers at about 4 % alpha, each masked by about 1,000 circles |
| Flocking | boids: separation, alignment, cohesion | draw trails, not dots, for a murmuration |
| Truchet, moiré, harmonograph | seeded tile rotation; two ring or line sets with a small offset; damped summed sines | no sourced numbers; measure the reference |

## Libraries

| Library | Gives | Runs in | Licence |
|---|---|---|---|
| perfect-freehand | variable-width stroke outline | Canvas 2D or SVG | MIT |
| rough.js | sketchy shapes, hachure fills, `seed` option | Canvas 2D or SVG | MIT |
| p5.brush | pencils, markers, hatching, watercolor fills, seedable | p5.js WEBGL or WebGL2 only | MIT |
| p5.grain | grain and texture overlays, seedable | p5.js | MIT |
| spectral.js | pigment mixing, also in GLSL | any | MIT |

Every recipe above also fits in a few lines of plain Canvas 2D. On a GPU, each surface layer becomes
a post pass (see the toolbox in [build.md](build.md)).

## Sources

- Kem, Glitch Cat Club, `ai-coding-skins` (MIT): `skins/studio/riso.js` (plates, halftone, offsets),
  `skins/thread/draw.js` (wobble, paper tile), https://github.com/Glitch-Cat-Club/ai-coding-skins
- lemo-opuscar `DIRECTOR.md` and `styles/*/STYLE.md` (MIT): halftone factor, hatch
  thresholds, swell, mark budget, boil policy, https://github.com/lemomo-ai/lemo-opuscar
- papermotion field notes and art direction (MIT): mass stack, depth layers, zoom line scale,
  https://github.com/francozanardi/papermotion
- iart-ai `javascript-animation-skills` `techniques.md` (MIT): shape with a darker rim, spot layers,
  https://github.com/iart-ai/javascript-animation-skills
- Tyler Hobbs essays on flow fields, soft textures, and watercolor, https://www.tylerxhobbs.com/words
- Inigo Quilez, domain warping, https://iquilezles.org/articles/warp/
- Karl Sims, reaction-diffusion tutorial, https://karlsims.com/rd.html
- Paul Bourke, Clifford attractors, https://paulbourke.net/fractals/clifford/
- Maxime Heckel, dithering, https://blog.maximeheckel.com/posts/the-art-of-dithering-and-retro-shading-web/
