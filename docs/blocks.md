# Blocks: the split map

This file maps the kit's skills into blocks: small units that each own one concern and combine into
a workflow for one task. It is a draft that changes as blocks are cut and used. `progress.md` track B
tracks the work. `skills/kit/blocks/blocks.toml` owns the block format that the loader reads today.
When this file and `blocks.toml` disagree, `blocks.toml` wins until a step in track B changes it.

Sources: the owner's direction of 2026-10-13, the composable-blocks council of 2026-10-13, and the
skill mining of 2026-10-09 (41 candidate blocks in 20 skills).

## The model

A task gets a workflow, and the workflow is a ledger written once at the start.

Example. The task is "walk me through what happens when I press Send". The plan block reads the
task and the `walk` template, then writes this ledger:

| Stage | Blocks | Runs | Ends when |
|---|---|---|---|
| start | glitch-walk start (its script) | one by one | the script prints `READY` |
| research | local-research ×2 | in parallel | each seat's notes file is done |
| plan the page | story, diagram | in parallel, then one merge | each section names its block |
| build | build-site, look-grammar, web-ui-basics | one by one | the page checks pass |
| view and talk | serve-local, annotate-loop | a loop with a cap | the owner writes `done`, or the loop stop fires |
| close | done-when-evidence | one by one | each done-when claim has evidence |

A different task uses the same blocks in a different ledger. "Make a site like this reference" uses
reference-analysis, look-grammar, story, motion, style-frames and build-site, and it skips the walk
start.

The parts and their owners:

- **Block.** One concern, its rules, its example and its checks. It names what it needs and what it
  gives by content, never by a step file or a skill.
- **Template.** The stage shape for one kind of task, such as `walk`, `site`, `question` or `change`.
  It fixes the required stages and the end. A skill such as `ink` or `research` becomes one or more
  templates plus its contract.
- **Plan block.** It reads the task, one template and the block catalog, then writes the ledger. It
  fills the optional blocks of each stage, with one reason line for each block it adds or leaves out.
  It cannot drop a stage that the template marks as required.
- **Ledger.** The step-run folder. `references/step_run.py` runs it. A step whose needs are done runs
  in the same turn as its siblings, and a step with an open need waits. So "in parallel or one by
  one" is the `Needs` column. A change is a re-plan folder that names its parent.
- **End condition.** A check that code can run: a line the owner wrote, a script result, a cap.
  "The model thinks it is done" is never an end condition.
- **Loop stop.** Every loop (review, view and talk) uses one rule. The cap is 3 rounds, keyed by a
  parent-loop key that fixes and re-plans inherit. The early stop compares open issues, not counts.
  `review_check.py start` owns this rule (progress.md B1, B2).

## Block format

Each block follows the Agent Skills format, so any tool that reads a `SKILL.md` can read a block,
and a block can later become a skill by a move.

```
skills/kit/blocks/<id>/
├── SKILL.md          # frontmatter + Rules, Example, Check (script), Check (judge)
├── references/       # item files, only for a pack
└── scripts/          # check scripts, only when the block ships one
```

```yaml
---
name: story
description: Turns a brief and its facts into a story sheet with beats, order and timing.
license: MIT
metadata:
  kind: step            # rule | pack | step
  owns: the order and the reveal of what the reader sees
  serves: plan the page, story
  needs: brief, facts
  gives: story sheet
  conflicts: ""
  users: ink, glitch-walk
---
```

- `metadata` values are strings in the Agent Skills spec, so a list is a comma-separated string.
- `kind: rule` is a rule card that a step copies in full. `kind: pack` is a domain pack with an
  index, item files under `references/`, a check, and a `license` line that says copy, point or
  install. `kind: step` does work and writes its `gives` artifact.
- Blocks live under `blocks/`, not `skills/`. The Agent Skills spec loads the name and description
  of every discovered skill at startup, so 40 blocks there would crowd the skill list, and selection
  falls as the list grows. Only the plan block and the loader read `blocks/`.
- The kit can become one Agent Plugins 1.0 package: a `plugin.json` at `skills/kit/`, skills under
  `skills/`, and `blocks/` beside them. Clients discover only `skills/` and `mcp.json`. A package path
  must resolve inside the package root, so no block may link outside the kit. Which of Claude Code,
  Codex and AGY load this package format natively is not checked yet.
- The 13 blocks today are flat files `blocks/<id>.md` with `Concept:`, `Serves:` and `Source:` lines.
  Moving them to folders with frontmatter is progress.md B16, and it changes the `flow_run.py`
  resolver and `blocks.toml` in the same step.

## Ownership and conflicts

1. A block owns one concern. Its `owns` line names that concern, and no other block's `owns` line
   names it.
2. When two files state the same rule, that is a defect. The fix is one owner and a pointer, never a
   `conflicts` line. Example: `red-first-check` repeats `prove-it-works`.
3. When two blocks are real alternatives, each names the other in `conflicts`. Code refuses a
   ledger that holds both, and the plan block picks one with a reason line. Examples:
   `html-diagram` and an SVG diagram block; the 300 ms rule in `web-ui-basics` and the 400-500 ms
   transitions.dev timings.
4. A block that names a step file is a defect, because a second workflow cannot use it. Example:
   `style-frames-concept.md:10` names flow's `02-design.md`.

## Catalog

Status: ✅ exists, ✂️ cut from a skill next, ⬜ later, 🆕 new, 🔒 stays inside its owner.

### Workflow blocks

| Block | Kind | Owns | Needs → gives | Today's text | Users | Status |
|---|---|---|---|---|---|---|
| plan | step | which blocks run, in which stage | task, template, catalog → ledger | none | every template | 🆕 B9 |
| frame | step | goal, done-when claims, out of scope | request → frame | flow 01-frame, research 01-question, ink intake | flow, research, ink | ⬜ |
| local-research | step | what local sources say | question → findings with paths | research 02-local, `references/fan-out.md` | research, glitch-walk, flow contracts | ⬜ |
| web-research | step | web evidence | query brief → findings with links | `skills/web-search` (stays whole; the block calls it) | research, reference-analysis, motion | ⬜ |
| reference-analysis | step | measured facts of a reference | references → measured notes with evidence levels | ink `references/references.md` passes 1-5 | ink, site builds, glitch-walk look | ✂️ ink |
| look-grammar | step | the look as tokens | measured notes → `DESIGN.md` tokens | `blocks/look-grammar.md` (ink copy) | ink, glitch-walk, any page | ✅ owner fix |
| story | step | order, beats, timing | brief, facts → story sheet | ink `story-and-motion.md` story sheet, structures, shot list, pacing | ink, glitch-walk, explainer pages | ✂️ ink |
| motion | pack | how things move | story sheet or UI states → timings, eases, one timing owner | emil pack (UI), ink `story-and-motion.md` transition catalogue and motion feel (story) | ink, any UI, sites | ✂️ B8 |
| diagram | rule set | when and how to draw | subject → diagram | `blocks/tech-diagram-concept`, `html-diagram`, `diagram-correctness`, `call-tree` | flow, research, glitch-walk, plans | ✅ |
| style-frames | step | "does it match the look" before the full build | tokens, page code → scored frames | `blocks/style-frames-concept.md` (ink copy) | ink, any visual build | ✅ owner fix |
| build-site | step | a page from tokens, story and motion | tokens, story, motion → page | ink `site.md`, `build.md` architecture | ink sites, glitch-walk, diagram pages | ✂️ ink |
| build-canvas | pack | drawn frames from time and one seed | tokens, story → scenes | ink `build.md`, `mark-making.md` | ink reels and plates | ✂️ ink |
| build-3d | pack | a 3D scene or hero | tokens, story → scene | ink `build.md` toolbox | ink, 3D sites | ⬜ after a second 3D task |
| headless-shot | step | how shots are taken | URL or file → shots | `blocks/headless-shot.md` (ink copy) | every visual check | ✅ owner fix |
| visual-qa | step | the visual verdict | shots, reference → pass or miss per check | ink `qa.md` reference match, frame and motion checks | ink, glitch-walk, any page | ✂️ ink |
| web-ui-basics | rule | the floor for one static page | page → pass or fail | `blocks/web-ui-basics.md` | any static page | ✅ HTML only |
| serve-local | step | a local view of the output | built folder → URL, stop command | none | glitch-walk, ink sites, any page | 🆕 |
| annotate-loop | step | comments from the owner to the agent | URL → comment file per round | none; Plannotator trial of 2026-10-12 | glitch-walk, any page | 🆕 |
| review | step | the review verdict | range → verdict | `skills/review-check` (stays whole; the block calls it) | flow, deliver | 🔒 |
| report | step | the written answer | findings → report with checked links | research 05-report, `references/plan-layout.md` first screen | research, council, reviews | ⬜ |
| done-when-evidence | rule | close maps claims to evidence | frame, results → evidence rows | `blocks/done-when-evidence.md` | every template | ✅ |

### Rule cards

| Block | Owns | Today's text | Status |
|---|---|---|---|
| first-principles | the rules every step keeps | `blocks/first-principles.md` | ✅ |
| behavior-claims | a frame as true-or-false claims | `blocks/behavior-claims.md` | ✅ |
| decision-with-default | a real fork with a marked default | `blocks/decision-with-default.md` | ✅ defect at `:8` vs `:27` |
| red-first-check | a check fails before the change | `blocks/red-first-check.md`, repeats `prove-it-works` | ✅ owner fix |
| evidence-label | one vocabulary for how sure a claim is | six vocabularies today: ink, web-search, research, prove-it-works, engineering-standards | 🆕 after one owner is picked |
| no-invented-quote | quote only text read, mark snippets | research, web-search, glitch-walk, decisions | ⬜ |
| link-check | a cited page says the claim | research 05-report | ⬜ |

### Stays whole

These are gates or control loops. A split would let a step claim the gate without its check. A
workflow calls them as one unit.

| Unit | Why |
|---|---|
| pair chunk loop | propose, approve, patch, check, commit is one gate the edit hook reads |
| deliver | the hands-off build loop and its stop |
| review-check core | units, checklist, owner map and verdict are one coverage gate |
| web-search depth gate and fetch ladder | the gate enforces the depth rule; the ladder is a policy of this one owner |
| council protocol | one sequence from Round 0 to cross-examination |
| decisions | capture gate, tiers and immutable body are one contract |
| step-run | status line, pick-up, close and wait are one machine |
| glitch-walk start | its script refuses, stops and gives exact words; it runs as one step |
| engineering-standards tables | each row already has an ID |
| ink seam law | one idea across three ink files; it moves with `story` |

## ink, split

ink becomes templates plus its contract. Its routes become the templates.

| ink step today | Blocks |
|---|---|
| 1 Intake | frame, web-research (when no references are given) |
| 2 Analyse the references | reference-analysis, look-grammar |
| 3 Story sheet, shots, transitions | story, motion (story route) |
| 4 Style frames | style-frames, headless-shot, visual-qa |
| 5 Animatic | build-site or build-canvas, flat pass |
| 6 Detail passes | build-canvas (mark-making) |
| 7 QA, export, handoff | visual-qa, build-canvas export |

| Template | Stages that run |
|---|---|
| `study` | intake, analyse, style frames |
| `prototype` | intake, analyse, story, style frames, animatic |
| `production` | all seven |
| `analysis-only` | intake, analyse |
| `site` | as `prototype`, with build-site and web-ui-basics |

ink's contract stays in its `SKILL.md`: match only inspected pixels, never copy a living artist's
exact style, keep story, look and motion apart, render from time and one seed.

## Order

The review stops and the ledger checks come first (progress.md B1-B3). Then:

1. Cut ink: reference-analysis, story, visual-qa, build-site and build-canvas get folders, and ink
   points to them. The three ink copies (look-grammar, style-frames, headless-shot) get one owner.
2. Give each cut block a second user in one live run: a glitch-walk page uses story, look-grammar,
   headless-shot and visual-qa. This is the live run that the earlier councils asked for.
3. Add serve-local and annotate-loop for the glitch-walk view-and-talk stage.
4. Write the plan block and the template format, then run one task through them by hand.
5. Cut the next skill, in the order of most blocks shared with what exists.

## Open questions

- Can the plan block build a ledger for a task with no template? The council's rule is: only after
  the same new kind of task has been done by hand three times.
- Which CLIs load an Agent Plugins 1.0 package natively?
- Is Plannotator enough for two-way comments, or does annotate-loop need a small comment server so
  replies show in the page?
