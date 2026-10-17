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
| plan the page | story, when-to-diagram | in parallel, then one merge | each section names its block |
| build | build-site, design-tokens, web-ui-basics | one by one | the page checks pass |
| view and talk | serve-local, annotate-loop | a loop with a cap | the owner writes `done`, or the loop stop fires |
| close | done-when-evidence | one by one | each done-when claim has evidence |

A different task uses the same blocks in a different ledger. "Make a site like this reference" uses
reference-analysis, design-tokens, story, motion, style-frames and build-site, and it skips the walk
start.

The parts and their owners:

- **Block.** One concern, its rules, its example and its checks. It names what it needs and what it
  gives by content, never by a step file or a skill.
- **Template.** The stage shape for one kind of task, such as `walk`, `site`, `question` or `change`.
  It fixes the required stages and the end. A skill such as `build` or `research` becomes one or more
  templates plus its contract.
- **Plan block.** It reads the task and the block catalog, then writes the ledger, with one reason
  line for each block it adds or leaves out. The owner sees these as the Chosen and Rejected
  tables before approval (`blocks/planner/SKILL.md` owns the rule; `plan_chart.py` checks and prints
  them). When a template fits, it starts from that template and may add any block to a stage, but it cannot drop a stage that the template marks as required. When
  no template fits, it builds the ledger from the catalog alone, and that ledger must still hold a
  frame, an end condition that code can check, and a close stage (owner decision, 2026-10-13).
- **Ledger.** The step-run folder. `references/step_run.py` runs it. A step whose needs are done runs
  in the same turn as its siblings, and a step with an open need waits. So "in parallel or one by
  one" is the `Needs` column. A change is a re-plan folder that names its parent.
- **End condition.** A check that code can run: a line the owner wrote, a script result, a cap.
  "The model thinks it is done" is never an end condition.
- **Loop stop.** Call `review_check.py start --parent-loop <key>`; the command owns the rule
  (progress.md B1, B2). `implement_run.py review` passes the saved key for implement and deliver.

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
  kind: step            # rule | pack | step | parent
  owns: the order and the reveal of what the reader sees
  serves: plan the page, story
  needs: brief, facts
  gives: story sheet
  conflicts: ""
  users: build, glitch-walk
---
```

- `metadata` values are strings in the Agent Skills spec, so a list is a comma-separated string.
- `kind: rule` is a rule card that a step copies in full. `kind: pack` is a domain pack with an
  index, item files under `references/`, a check, and a `license` line that says copy, point or
  install. `kind: step` does work and writes its `gives` artifact.
- `kind: parent` groups parts under shared rules; `blocks/blocks.toml` owns the format,
  `metadata.parts`, and implement selection.
- Blocks live under `blocks/`, not `skills/`. The Agent Skills spec loads the name and description
  of every discovered skill at startup, so 40 blocks there would crowd the skill list, and selection
  falls as the list grows. A skill points to the blocks it uses by a path from the kit root (`blocks/<id>/SKILL.md`), and
  says once that the kit root is two folders up from the skill's folder. This holds in the plugin copy
  that each CLI installs, but not through a link to one skill folder, because a Read tool resolves
  `..` on the link path (see Packaging). The implement loader copies the blocks that a catalog line names into the
  step files. No CLI lists a block as a skill.
- The kit is one Agent Plugins 1.0 package: a `plugin.json` at `skills/kit/`, skills under
  `skills/`, and `blocks/` beside them. Clients discover only `skills/` and `mcp.json`. A package path
  must resolve inside the package root, so no block may link outside the kit. See Packaging below
  for what each CLI loads.
- Every kit block is a folder with this frontmatter (progress.md B16, done 2026-10-13). The implement
  loader finds blocks by folder, writes `description` to the step as `Concept:`, and refuses a
  `name` that is not the folder. A test checks every block's frontmatter.

## Ownership and conflicts

On 2026-10-17, the vendor modal-duration conflict closed under `blocks/ui-motion/SKILL.md`;
the vendor pack remains unchanged.

On 2026-10-17, the owner settled the source-of-truth part of #70 in `blocks/base-rules/SKILL.md`;
research and web-search retain their evidence owners.

On 2026-10-17, the owner settled the look-round part of #70 in `blocks/look-check/SKILL.md`.

On 2026-10-17, the owner settled the evidence part of #70 with three separate owners:
`blocks/prove-it-works/SKILL.md`, `blocks/cite-sources/SKILL.md`, and `blocks/done-when-evidence/SKILL.md`.

On 2026-10-17, C6 closed with `blocks/web-ui-basics/SKILL.md` as the check-width owner;
diagram and page-shot blocks point to it.

On 2026-10-17, the owner settled the voice-file part of #70 in `blocks/voice-review/SKILL.md`;
`voice-pr` points to that owner.

On 2026-10-17, the owner chose `blocks/worker-brief/SKILL.md` as the owner of gates with no user present.

On 2026-10-17, the owner approved domain selection by `implement_run.py start --domains`;
`skills/kit/skills/implement/SKILL.md` names the command and its saved choice.

1. A block owns one concern. Its `owns` line names that concern, and no other block's `owns` line
   names it.
2. When two files state the same rule, that is a defect. The fix is one owner and a pointer, never a
   `conflicts` line. Example: `red-first-check` repeated `prove-it-works`, so on 2026-10-13 both
   texts became the one block `prove-it-works`, and the skill points to it.
3. When two blocks are real alternatives, each names the other in `conflicts`. Code refuses a
   ledger that holds both, and the plan block picks one with a reason line.
4. A block names a step by its role. A workflow's file name may follow as an example, as
   `call-tree/SKILL.md` does for the impact step (`04-impact.md` in implement).

On 2026-10-17, the owner closed C4/D1 with the diagram form rule in
`references/explain-formats.md`. The plan chart still refuses parent ids until B14 shares
the resolver; it still accepts part ids without loading the parent.

## Catalog

Status: ✅ exists, ✂️ cut from a skill next, ⬜ later, 🆕 new, 🔒 stays inside its owner.

### Workflow blocks

| Block | Kind | Owns | Needs → gives | Today's text | Users | Status |
|---|---|---|---|---|---|---|
| planner | step | which blocks a task uses, in which order, with which inputs | task → `plan.md`, a step table with a Block and a Why column | `blocks/planner` | the `plan` skill | ✅ basic, B9 |
| plan-chart | step | the check and the drawing of a plan's step graph | `plan.md` → `plan.html` or the first defect | `blocks/plan-chart` with `scripts/plan_chart.py` | the `plan` skill | ✅ basic |
| plan-review | step | the owner's approval of a plan before work starts | `plan.html`, `plan.md` → approved plan, started step run | `blocks/plan-review` with `scripts/start_plan.py`, which refuses a plan that changed after approval | the `plan` skill | ✅ basic |
| comment-round | rule | one Plannotator round: command, saved result with file hashes, answers, loop stop | files → `<N>.json`, `<N>-answers.md` | `blocks/comment-round` with `scripts/comment_round.py` | plan-review, page-comments | ✅ basic |
| page-comments | step | the comment, fix and check loop for a built page | page, its checks, its build block → approved page | `blocks/page-comments` | glitch-walk, any page | ✅ basic |
| change-review | step | that a plan's commits get the full review-check verdict | range → verdict | `blocks/change-review`, which runs `skills/review-check` whole | plans that make commits | ✅ basic |
| voice-reply | rule | the final reply to the owner, by the session's reply rules | step results → reply | `blocks/voice-reply` (points to the agent instructions) | the last step of a plan | ✅ basic |
| voice-teach | rule | the order of ideas in an explanation to learn from | subject, known terms → explanation | `blocks/voice-teach` | explain, the last step of an explain plan | ✅ basic |
| voice-pr | rule | how PR titles and bodies read | change, tests → PR draft | `blocks/voice-pr` | the last step of a PR plan | ✅ basic |
| voice-review | rule | how review comments sound | findings → comments in the reviewer's voice | `blocks/voice-review`, which owns the voice file path and privacy rule | the last step of a review plan | ✅ basic |
| frame | step | goal, done-when claims, out of scope | request → frame | implement 01-frame, research 01-question, build intake | implement, research, build | ⬜ |
| local-research | step | what local sources say | question → findings with paths | `blocks/local-research` | research (planned: glitch-walk, implement contracts) | ✅ |
| web-research | step | web evidence | query brief → findings with links | `skills/web-search` (stays whole) with `blocks/deep-search` and `blocks/cite-sources` | research, reference-analysis, motion | ✅ by the skill |
| primary-sources | step | the origin of a claim: paper, spec, or the author's code at the commit | question → origin rows (source, exact location, quote) | `blocks/primary-sources` | research, web-search | ✅ |
| deep-search | rule | how a search goes past the known answers | question, known answers → sub-questions, seeds, off-path ideas | `blocks/deep-search` | research, web-search | ✅ |
| tool-trial | step | the hands-on test of tool candidates | tool question → candidate table | `blocks/tool-trial` | research (planned: implement contracts) | ✅ |
| reference-analysis | step | measured facts of a reference | references → measured notes with evidence levels | `blocks/reference-analysis` | build, design-tokens | ✅ |
| design-tokens | step | the look as tokens | measured notes → `DESIGN.md` tokens | `blocks/design-tokens` (owner of the token rules) | build, glitch-walk, any page | ✅ owner fix |
| story | step | order, beats, key frames, pacing | brief, facts → story sheet, shot list | `blocks/story` | build | ✅ |
| motion | pack | how things move | story sheet or UI states → timings, eases, one timing owner | `blocks/ui-motion` (UI, one timing owner, emil pack tables); build `story-and-motion.md` transition catalogue and motion feel (story) | build, any UI, sites | ✅ basic UI, B8 |
| diagram | parent | shared diagram rules | subject or diagram → rules for selected parts | `blocks/diagram` with parts `when-to-diagram`, `html-diagram`, `call-tree`, `diagram-check`; `references/explain-formats.md` owns when and which form | implement; glitch-walk and plans use the parts | ✅ slice 1 |
| style-frames | step | "does it match the look" before the full build | tokens, page code → scored frames | `blocks/look-check` (owner of reference scoring) | implement, build | ✅ |
| build-site | step | what changes when the piece is a website | look, story → page | `blocks/build-site` | build | ✅ |
| build-canvas | pack | a drawn piece rendered from time and one seed | look, story, shot list → piece | `blocks/build-canvas` with `references/mark-making.md` | build | ✅ |
| build-3d | pack | a 3D scene or hero | tokens, story → scene | build `build.md` toolbox | build, 3D sites | ⬜ after a second 3D task |
| headless-shot | step | how shots are taken | URL or file → shots | `blocks/headless-shot` (owner of the shot and compare commands) | implement, build | ✅ |
| visual-qa | step | the visual verdict and the QA log | frames, playback → QA log | `blocks/visual-qa` | build | ✅ |
| web-ui-basics | rule | the floor for one static page | page → pass or fail | `blocks/web-ui-basics` | any static page | ✅ HTML only |
| serve-local | step | a local view of the checked build | built folder → URL, commit, stop command | `blocks/serve-local` | page-comments, build sites, any page | ✅ basic |
| review | step | the review verdict | range → verdict | `skills/review-check` (stays whole; `blocks/change-review` calls it) | implement, deliver, plans | 🔒 |
| report | step | the written answer | findings → report with checked links | `blocks/report` (report sections; the agent instructions own the reply) | research | ✅ |
| done-when-evidence | rule | close maps claims to evidence | frame, results → evidence rows | `blocks/done-when-evidence` | every template | ✅ |

### Rule cards

| Block | Owns | Today's text | Status |
|---|---|---|---|
| base-rules | the rules every step keeps | `blocks/base-rules` | ✅ |
| requirements | a frame as true-or-false claims (was behavior-claims) | `blocks/requirements` | ✅ |
| scratch-home | trial and test state in a temporary home, apart from live settings | `blocks/scratch-home` | ✅ basic |
| decision-with-default | a real fork with a marked default, and the trade-off form | `blocks/decision-with-default` (engineering-standards section 5 points here) | ✅ defect fixed |
| prove-it-works | what proves a change works | `blocks/prove-it-works` (red-first-check merged in; the skill points here) | ✅ |
| checked-units | small units that each end in a check | `blocks/checked-units` (the skill sequence-verifiable-units points here) | ✅ |
| when-to-extract | code stays inline without a reason | `blocks/when-to-extract` (the skill minimize-reader-load points here) | ✅ |
| worker-brief | what a worker brief holds, how a worker acts, and what a skill does at an owner gate with no user | `blocks/worker-brief` | ✅ |
| run-workers | how a root session runs seats on its host | `blocks/run-workers` (the three adapters point here) | ✅ |
| task-file | the task file and the resume | `blocks/task-file` | ✅ |
| cite-sources | each claim cites its owning source; quote only text read; mark snippets | `blocks/cite-sources` | ✅ |
| link-check | a cited page says the claim | `blocks/link-check` | ✅ |

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
| `build` skill seam law | one idea across three files in the `build` skill; it moves with `story` |
| cleanup-gate pass | the six emissions in order are one gate; a step that emitted nothing did not run |
| review-walk steps | each step needs the reviewer, and the map, comments and walk share one run |
| implement step graph | the step list, status and catalog are one machine that `implement_run.py` runs |

## Every skill, split (2026-10-13)

Each skill keeps its contract, its step table and its stays-whole part, and points to the blocks
below. A skill that was one rule is now a block, and its skill folder holds one pointer, so every
path that names the skill still works.

| Skill | Blocks it points to | Stays in the skill |
|---|---|---|
| prove-it-works | prove-it-works | the pointer only |
| sequence-verifiable-units | checked-units | the pointer only |
| minimize-reader-load | when-to-extract | the pointer only |
| engineering-standards | decision-with-default (trade-off form) | the rule tables and the procedure |
| pair | task-file, checked-units, when-to-extract | the chunk loop, its guards, commit per behavior |
| deliver | task-file, checked-units, prove-it-works | the predicate loop, the commit gate, the review stop |
| research | local-research, deep-search, primary-sources, cite-sources, tool-trial, report, link-check, worker-brief (no-user gate) | the step table, the web seat triggers and timing |
| web-search | deep-search, primary-sources, cite-sources, link-check, worker-brief, run-workers | seats, site sets, reach tools, Yield and Floor, the depth gate, the merge |
| council | worker-brief, run-workers | the protocol, the convergence check, the output, the decision log |
| orchestrate-claude, -codex, -agy | run-workers, worker-brief | the lines that differ by runtime |
| review-check | when-to-extract, worker-brief (no-user gate) | the whole review gate |
| review-walk | voice-review | the five steps and GitHub write gates (stays whole) |
| cleanup-gate | when-to-extract (by its rules file) | the six-step pass (stays whole) |
| decisions | none | the whole capture contract (stays whole) |
| implement | prove-it-works, checked-units, worker-brief (no-user gate), and every block its catalog lines name | the step graph and the loader |
| build | reference-analysis, story, look-check, headless-shot, visual-qa, build-site (which points to design-tokens), build-canvas, web-ui-basics | the contract, the routes, the seam law, the reel rules |
| explain | voice-teach, worker-brief (no-user gate) | the pointer to the block and `references/explain-formats.md` |

Plans that stay open: research writes the ledger for questions, with `web needs local` only when
local findings set the query (the owner's local-then-web example); the emil pack owns UI motion and
the `build` skill owns story motion (B8).

## The `build` skill, split

The `build` skill becomes templates plus its contract. Its routes become the templates.

| Step in the `build` skill today | Blocks |
|---|---|
| 1 Intake | frame, web-research (when no references are given) |
| 2 Analyse the references | reference-analysis, design-tokens |
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

The `build` skill's contract stays in its `SKILL.md`: keep story, look and motion apart, prove the
look and the story before production, and render from time and one seed. Reference rules point to
`reference-analysis`.

### Where each old section of the `build` skill went (2026-10-13)

| Old section | Now |
|---|---|
| `SKILL.md` contract: inspected pixels, evidence labels, borrow not copy, match density | `blocks/reference-analysis` |
| `references.md` Intake | stays; the benchmark line and the inspection rules moved to `blocks/reference-analysis` |
| `references.md` Research references, Pass 1-5, Evidence language | `blocks/reference-analysis` |
| `references.md` Style grammar template | stays |
| `story-and-motion.md` Story sheet, Structures, Shot list, Pacing | `blocks/story` |
| `story-and-motion.md` Drivers (its settled-state line points to `blocks/web-ui-basics`), Transition catalogue, Seam law, Scene graph, Timeline, Motion feel, Web-native media ideas, Seam design | stay |
| `qa.md` Required evidence | stays; the contact-sheet rule moved to `blocks/visual-qa` |
| `qa.md` Reference match | scoring to `blocks/look-check`, compare command to `blocks/headless-shot`; the `build` skill's example stays |
| `qa.md` Story checks | stay; key frame and pacing checks moved to `blocks/story`; the scroll-story check points to the settled-state rule of `blocks/web-ui-basics` |
| `qa.md` Frame checks, Motion checks | `blocks/visual-qa`; mass, paper, seam law, boil and loop seam checks stay |
| `qa.md` Automated checks | stay; hidden content points to `blocks/web-ui-basics` |
| `qa.md` QA log | `blocks/visual-qa`; the `build` skill's step names stay |
| `site.md` (whole file, removed) | `blocks/build-site`; DESIGN.md rules to `blocks/design-tokens`, page frames to `blocks/look-check`, page floor to `blocks/web-ui-basics` |
| `build.md` Architecture, Determinism, Debug modes, Performance, Toolbox, Delivery, Technical references | `blocks/build-canvas` |
| `build.md` Style frames, Animatic, Frame export, Handoff | stay; the shot command points to `blocks/headless-shot` |
| `mark-making.md` (whole file) | `blocks/build-canvas/references/mark-making.md` |

## Order

The review stops and the ledger checks come first (progress.md B1-B3). Then:

1. Cut build: reference-analysis, story, visual-qa, build-site and build-canvas get folders, and build
   points to them. The three build copies (design-tokens, style-frames, headless-shot) get one owner.
2. Give each cut block a second user in one live run: a glitch-walk page uses story, design-tokens,
   headless-shot and visual-qa. This is the live run that the earlier councils asked for.
3. Add serve-local and annotate-loop for the glitch-walk view-and-talk stage.
4. Write the plan block and the template format, then run one task with a template and one without
   by hand.
5. Cut the next skill, in the order of most blocks shared with what exists. Done for every skill on
   2026-10-13 (see "Every skill, split"), at the owner's word, before steps 2-4.

## annotate-loop is strict

The 2026-10-12 glitch walk had no comment round. The `plannotator` binary was only in the trial's
scratch folder, not on the PATH, and no glitch-walk step called it, so nothing ran and nothing
reported the gap. Plannotator is the tool (owner decision, 2026-10-13); no comment server is built.

1. The `walk` template marks view-and-talk as required, so the plan block cannot drop it.
2. Before the stage starts, a script runs `command -v plannotator`. When it fails, the step status is
   `unavailable plannotator` and the run stops and tells the owner. It never skips the stage.
3. Each round writes Plannotator's JSON output to `annotate/<round>.json` in the run folder. The
   step closes only when the newest file holds the owner's `done`, or the loop stop fires.
4. The agent's answer to each comment goes in the step file beside the comment's text quote, because
   the page closes on submit and does not show replies.

## Packaging

On 2026-10-13 a probe package (root `plugin.json`, `skills/`, `mcp.json`) was tested in each CLI:

| CLI | Root `plugin.json` | Skills from `skills/` | MCP file it reads |
|---|---|---|---|
| Codex 0.162.1 | read, spec name rule enforced | installed and enabled | `mcp.json`, as the spec says |
| Claude Code 2.1.296 | not read; uses `.claude-plugin/plugin.json` | found | `.mcp.json` |
| AGY 1.3.4 | required | found | `mcp_config.json` |

So the kit can ship as a skills-only package that all three load. An MCP server would need one
file per CLI, made by one script.

A second probe on the same day put a `blocks/probe/SKILL.md` with a random code word beside a skill
that says to read `../../blocks/probe/SKILL.md`. Each CLI installed a copy of the whole package
(Claude Code and Codex under `plugins/cache/<marketplace>/<plugin>/<version>/`, AGY under
`~/.gemini/config/plugins/<plugin>/`), and a headless model run in each read the block from that copy
and gave the code word. Headless AGY needed a `read_file(<folder>)` allow rule, and it was told to
use only its file-read tool. So the kit ships as the plugin `swarm` (`skills/kit/plugin.json`,
`skills/kit/.claude-plugin/plugin.json`, and the marketplace `.claude-plugin/marketplace.json` at the
repo root).

A pane trial on the same day installed the real kit from this repo in each CLI and opened one
interactive pane per CLI. Each model loaded a kit skill from the plugin, followed its pointer, and
quoted the block's first rule. Claude Code loaded the plugin from the marketplace folder itself (the
checkout), not from its cache copy, so a kit edit reaches it at once; Codex and AGY read their copies.
Claude Code refused `prove-it-works`, which has `disable-model-invocation: true`; Codex and AGY
loaded it. The research report is in the owner's private reports folder.

## Open questions

- How many required stages does a ledger with no template need beyond frame, end condition and close?
- Who installs `plannotator` on the PATH, and from which release, on each Mac?
