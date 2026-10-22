---
name: build
description: Make pictures, films, and sites where code (p5.js, three.js, shaders) draws every frame. Use when the user asks for a reel, loop, generative still, landing page, portfolio, "make this in code", or style rules from references only.
---

# build

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

- Find, inspect, measure, and label every reference, and take only what may be taken, as `blocks/reference-analysis/SKILL.md` says. The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.
- Prove the look with style frames and the story with an animatic, both before production.
- Apply the architecture and deterministic-rendering rules in `blocks/build-canvas/SKILL.md`.
  The look tokens follow `blocks/design-tokens/SKILL.md`, with the shared rules in `blocks/web-ui/SKILL.md`.
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
2. **Analyse the references** with `blocks/reference-analysis/SKILL.md`, and write
   `ink/02-look.md` with the style grammar in [references.md](references/references.md).
3. **Story sheet, shot list, transitions, scene graph.** See `blocks/story/SKILL.md`
   and [story-and-motion.md](references/story-and-motion.md).
4. **Style frames** with the real drawing code, one of them the signature shot, scored beside the
   reference as `blocks/look-check/SKILL.md` says, including its round rule. See
   `blocks/build-canvas/references/mark-making.md` and [qa.md](references/qa.md).
5. **Animatic.** Flat shapes, final timing, real transitions. See [build.md](references/build.md).
6. **Detail passes** up to the style frames, with scenes in batches of about eight. Build with `blocks/build-canvas/SKILL.md` and its `references/mark-making.md`.
7. **QA, export, and handoff.** See `blocks/visual-qa/SKILL.md`, [qa.md](references/qa.md),
   and [build.md](references/build.md).

A website changes most steps: sections replace shots, screen widths replace fps, and a `DESIGN.md`
joins the look. Use `blocks/build-site/SKILL.md` from intake on when the form is a website.

A pattern file holds the mechanics of one reusable structure:
[recursive-zoom.md](references/patterns/recursive-zoom.md) covers worlds inside worlds. Add a
pattern file when a build solves a new one.

## Progress

Choose `<piece>`, the piece folder, by `references/run-folder.md`, so a new chat continues where
the last one stopped.

`start` binds `{{site}}` and `{{shots}}` as `blocks/blocks.toml` says.

Example. `~/work/seed-city/ink/` holds one file per step. Steps 1-4 are done, so line 1 of
`04-style-frames.md` reads `Status: done 3f9a1c2b7e10`. A new chat gets
`build continue ~/work/seed-city`, reads line 1 of every file, and starts `05-animatic.md`.

A piece is a step run, so `references/step-run.md` owns the status line, ready and stale steps,
pick-up, close, and when a step waits for the user. Run the front door in `skills/plan/SKILL.md`
with the preset below, and write `plan.md` in `<piece>/ink/`. The `Blocks` column makes `start`
write each block's rules into its step file.
Each file holds the step's output, not a copy of it.

## Preset

Stop: owner
Offers: build-site, design-tokens, web-ui-basics, ui-motion, serve-local

| File | Needs | Holds | Blocks | Why |
|---|---|---|---|---|
| `01-intake.md` | none | story, references, benchmark, form, size, limits, and delivery | reference-analysis | intake finds and labels the references |
| `02-look.md` | intake | reference analysis and style grammar | reference-analysis | the look comes from measured references |
| `03-story.md` | intake | story sheet, shot list, transitions, and scene graph | story | the story lives apart from the look |
| `04-style-frames.md` | look, story | style frames, compare images, and scores | look-check, headless-shot, visual-qa | style frames prove the look before production |
| `05-animatic.md` | story | animatic and timing | build-canvas | the animatic proves the story before production |
| `06-detail.md` | style-frames, animatic | detail passes and frame paths | build-canvas | detail passes bring scenes up to the frames |
| `07-export.md` | style-frames, detail | QA log, export, and handoff | build-canvas, headless-shot, visual-qa, voice-reply | inspected frames and the handoff reach the owner |

For a website, the planner puts `build-site` in place of `build-canvas`, as the Steps section says.

- A build step lists its frame paths and a hash of its source files in its body, so a code change
  changes its revision.
- Intake runs `skip <piece>/ink/ <step> <route reason>` with the script in `references/step-run.md`
  for each skipped step from the route. `study` skips 3, 5, and 6. `prototype` skips 6.
  `analysis-only` skips 4 to 7. `production` skips none.

## Roles

- For a missing builder or visual critic seat, apply the seat routes in `blocks/worker-brief/SKILL.md`.
- Steps 1 to 3 run in the main session, because they need the user's answers.
- Steps 4 to 7 go to the role `build`. The builder scores its own rounds. When it closes a step
  by `references/step-run.md`, the main session sends the final compare images to the role `review.visual`, and a `miss`
  from that critic reopens the step.
  [orchestration.json](orchestration.json) declares both roles, and the runtime adapter resolves
  each one with its fallbacks.
- Before two builder seats run at once, each owns its files in a named subfolder; `05-animatic`
  may run beside `04-style-frames` by the pick-up rule in the kit's `references/step-run.md`.
- A builder writes only the piece folder and its own step file, and it closes the step with the
  `done` command in `references/step-run.md`. The main session inspects the frames itself before
  it accepts the step.
- A new skill-specific role is one route in the roles registry and one line in
  `orchestration.json`.

## Effort and cost

The `build` route sets the model and the reasoning effort. A production reel takes hours of
render, look, and fix loops. One public 26-scene rebuild took about 5 h 40 min and 716,000
output tokens. Tell the user the likely cost before production.

## Output

Never return only a prompt. Return the build, the run command, the seed, and the step files. Say what
matched, what changed to stay original, and what was not verified.
