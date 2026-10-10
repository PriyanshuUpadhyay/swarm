---
name: ink
description: Draw pictures, generative plates, short films, and websites where every frame is made by code (Canvas 2D, p5.js, three.js, WebGL or WebGPU shaders, 3D models), from a story, a visual reference, or both. Use for a story reel, animated short, explainer, title sequence, loop, generative still or plate, dense comic or print-style illustration, or infinite zoom; for a landing page, portfolio, or product site with a 3D hero, a loader, or scroll motion; for "make a website like this", "make this in p5.js", "animate this story", "storyboard and code this", "draw every frame in JavaScript", "creative coding from this reference", "recreate this style in code", or "animate this like the reel"; and for extracting style, story, and motion rules from references without building. Keeps story, look, and motion apart, proves the look with style frames beside the reference, and proves the story with an animatic.
---

# ink

Every frame is drawn by code, from a story, a reference, or both. Three layers change for their own
reasons, so each one lives in its own place:

| Layer | Answers | Lives in |
|---|---|---|
| Story | what happens, in what order, for how long | `ink/03-story.md` and the shot list in code |
| Look | palette, light, line, tone, texture | `ink/02-look.md` and one `look` token object |
| Motion | camera, transitions, local motion, boil | `ink/03-story.md`, scene and transition modules |

Example. "A paper boat survives a storm" has five beats. The look comes from the user's watercolour
reference. A whip pan carries the boat into the storm, and a match cut turns its sail into the
sunrise. A neon restyle changes only the look tokens.

## Contract

- Match a look only from pixels you inspected yourself. When the brief has no references,
  research them from the brief. Keep evidence levels apart: `visible`, `stated`, `inferred`,
  `unknown`.
- Borrow techniques and high-level traits. Never copy characters, logos, signatures, exact
  compositions, or a living artist's exact personal style.
- Match density, not only palette. Measure the reference, write the numbers down, and meet them.
- Prove the look with style frames and the story with an animatic, both before production.
- Keep the layers apart. Scene code has no colour literals and no story timing.
- Render every frame from time and one seed (`?t=`). A stateful simulation steps at a fixed rate
  and keeps checkpoints.
- Choose tools for the look and the story. Dependencies, load time, 3D, and WebGPU are fine.
- Inspect rendered frames at each milestone. Code that runs does not prove the picture works.

## Routes

- `study`: one technique, transition, plate, or style frame.
- `prototype`: three style frames and the animatic.
- `production`: the full look, texture, lettering, export, and handoff.
- `analysis-only`: rules from references, with no build.

Default to `prototype` for anything with more than one beat. A polished multi-scene piece is not a
one-prompt task, so do not suggest it is.

## Steps

1. **Intake.** Story, references (researched from the brief when none are given) with a
   benchmark, a start, middle, and end frame per shot, form, size, fps, limits, and delivery. See
   [references.md](references/references.md).
2. **Analyse the references** and write `ink/02-look.md` with measured numbers. See
   [references.md](references/references.md).
3. **Story sheet, shot list, transitions, scene graph.** See
   [story-and-motion.md](references/story-and-motion.md).
4. **Style frames** with the real drawing code, one of them the signature shot, scored beside the
   reference for at least two rounds. See [mark-making.md](references/mark-making.md) and
   [qa.md](references/qa.md).
5. **Animatic.** Flat shapes, final timing, real transitions. See [build.md](references/build.md).
6. **Detail passes** up to the style frames, with scenes in batches of about eight. See
   [mark-making.md](references/mark-making.md).
7. **QA, export, and handoff.** See [qa.md](references/qa.md) and [build.md](references/build.md).

A website changes most steps: sections replace shots, screen widths replace fps, and a `DESIGN.md`
joins the look. Read [site.md](references/site.md) at intake when the form is a website.

A pattern file holds the mechanics of one reusable structure:
[recursive-zoom.md](references/patterns/recursive-zoom.md) covers worlds inside worlds. Add a
pattern file when a build solves a new one.

## Progress

A piece keeps its progress in its own folder, so a new chat continues where the last one stopped.

Example. `~/work/seed-city/ink/` holds one file per step. Steps 1-4 are done, so line 1 of
`04-style-frames.md` reads `Status: done 3f9a1c2b7e10`. A new chat gets
`ink continue ~/work/seed-city`, reads line 1 of every file, and starts `05-animatic.md`.

- Step N writes `ink/0N-<name>.md`: `01-intake`, `02-look`, `03-story`, `04-style-frames`,
  `05-animatic`, `06-detail`, `07-export`. The file is the step's output (notes, grammar, story
  sheet, scores, QA log), not a copy of it.
- A piece is a step run, so the kit's `references/step-run.md` owns the status line, ready and
  stale steps, pick-up, close, and when a step waits for the user.
- Step 2 uses 1, 3 uses 1, 4 uses 2 and 3, 5 uses 3, 6 uses 4 and 5, and 7 uses 4 and 6.
- A build step lists its frame paths and a hash of its source files in its body, so a code change
  changes its revision.
- Intake sets the skipped steps from the route. `study` skips 3, 5, and 6. `prototype` skips 6.
  `analysis-only` skips 4 to 7. `production` skips none.

## Roles

- Steps 1 to 3 run in the main session, because they need the user's answers.
- Steps 4 to 7 go to the role `ink.build`. The builder scores its own rounds. When it sets a step
  done, the main session sends the final compare images to the role `review.visual`, and a `miss`
  from that critic reopens the step.
  [orchestration.json](orchestration.json) declares both roles, and the runtime adapter resolves
  each one with its fallbacks.
- Before two builder seats run at once, each owns its files in a named subfolder; `05-animatic`
  may run beside `04-style-frames` by the pick-up rule in `references/step-run.md`.
- A builder writes only the piece folder and its own step file, and it ends by setting line 1. The
  main session inspects the frames itself before it accepts the step.
- A new skill-specific role is one route in the roles registry and one line in
  `orchestration.json`.

## Effort and cost

The `ink.build` route sets the model and the reasoning effort. A production reel takes hours of
render, look, and fix loops. One public 26-scene rebuild took about 5 h 40 min and 716,000
output tokens. Tell the user the likely cost before production.

## Output

Never return only a prompt. Return the build, the run command, the seed, and the step files. Say what
matched, what changed to stay original, and what was not verified.
