# Blocks: the catalog

The split map is in [blocks.md](blocks.md).

Status: ✅ exists, ✂️ cut from a skill next, ⬜ later, 🆕 new, 🔒 stays inside its owner.

## Workflow blocks

| Block | Kind | Owns | Needs → gives | Today's text | Users | Status |
|---|---|---|---|---|---|---|
| planner | step | which blocks a task uses, in which order, with which inputs | task → `plan.md`, a step table with a Block and a Why column | `blocks/planner` | the `plan` skill | ✅ basic, B9 |
| plan-chart | step | the check and the drawing of a plan's step graph | `plan.md` → Mermaid chart in `plan.md`, `plan.html`, or the first defect | `blocks/plan-chart` with `scripts/plan_chart.py` | the `plan` skill | ✅ basic |
| plan-review | step | the owner's approval of a plan before work starts | `plan.html`, `plan.md` → approved plan, started step run | `blocks/plan-review` with `scripts/start_plan.py`, which refuses a plan that changed after approval | the `plan` skill | ✅ basic |
| comment-round | rule | one Plannotator round: command, saved result with file hashes, answers, loop stop | files → `<N>.json`, `<N>-answers.md` | `blocks/comment-round` with `scripts/comment_round.py` | plan-review, page-comments | ✅ basic |
| page-comments | step | the comment, fix and check loop for a built page | page, its checks, its build block → approved page | `blocks/page-comments` | glitch-walk, any page | ✅ basic |
| change-review | step | that a plan's commits get the full review check verdict | range → verdict | `blocks/change-review`, which runs `skills/review/references/check.md` whole | plans that make commits | ✅ basic |
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
| voice | parent | shared voice rules | kind of output → rules for selected parts | `blocks/voice` with parts `voice-pr`, `voice-reply`, `voice-review`, `voice-teach` | planner, explain | ✅ |
| sources | parent | shared source rules | question or claims → rules for selected parts | `blocks/sources` with parts `primary-sources`, `deep-search`, `cite-sources`, `link-check` | research, web-search | ✅ |
| web-ui | parent | shared web page rules | page or its code → rules for selected parts | `blocks/web-ui` with parts `web-ui-basics`, `ui-motion`, `design-tokens`, `headless-shot`, `look-check` | implement, build, planner | ✅ |
| plan-flow | parent | shared plan and comment-round rules | plan or built page → rules for selected parts | `blocks/plan-flow` with parts `planner`, `plan-chart`, `plan-review`, `comment-round`, `page-comments` | planner | ✅ |
| style-frames | step | "does it match the look" before the full build | tokens, page code → scored frames | `blocks/look-check` (owner of reference scoring) | implement, build | ✅ |
| build-site | step | what changes when the piece is a website | look, story → page | `blocks/build-site` | build | ✅ |
| build-canvas | pack | a drawn piece rendered from time and one seed | look, story, shot list → piece | `blocks/build-canvas` with `references/mark-making.md` | build | ✅ |
| build-3d | pack | a 3D scene or hero | tokens, story → scene | build `build.md` toolbox | build, 3D sites | ⬜ after a second 3D task |
| headless-shot | step | how shots are taken | URL or file → shots | `blocks/headless-shot` (owner of the shot and compare commands) | implement, build | ✅ |
| visual-qa | step | the visual verdict and the QA log | frames, playback → QA log | `blocks/visual-qa` | build | ✅ |
| web-ui-basics | rule | the floor for one static page | page → pass or fail | `blocks/web-ui-basics` | any static page | ✅ HTML only |
| page-shelf | step | how a built page reaches the owner | page folder → shelf URL, hash, source, commit | `blocks/page-shelf` | page-comments, planner, any static page | ✅ basic |
| serve-local | step | a local view of the checked build | built folder → URL, commit, stop command | `blocks/serve-local` | page-comments, build sites, a page that needs a dev server | ✅ basic |
| review | step | the review verdict | range → verdict | `skills/review` check mode (stays whole; `blocks/change-review` calls it) | implement, plans | 🔒 |
| report | step | the written answer | findings → report with checked links | `blocks/report` (report sections; the agent instructions own the reply) | research | ✅ |
| done-when-evidence | rule | close maps claims to evidence | frame, results → evidence rows | `blocks/done-when-evidence` | every template | ✅ |

## Rule cards

| Block | Owns | Today's text | Status |
|---|---|---|---|
| base-rules | the rules every step keeps | `blocks/base-rules` | ✅ |
| requirements | a frame as true-or-false claims (was behavior-claims) | `blocks/requirements` | ✅ |
| scratch-home | trial and test state in a temporary home, apart from live settings | `blocks/scratch-home` | ✅ basic |
| decision-with-default | a real fork with a marked default, and the trade-off form | `blocks/decision-with-default` (engineering-standards section 5 points here) | ✅ defect fixed |
| prove-it-works | what proves a change works | `blocks/prove-it-works/SKILL.md` (red-first-check merged in) | ✅ |
| checked-units | small units that each end in a check | `blocks/checked-units/SKILL.md` | ✅ |
| when-to-extract | code stays inline without a reason | `blocks/when-to-extract/SKILL.md` | ✅ |
| worker-brief | what a worker brief holds, how a worker acts, and what a skill does at an owner gate with no user | `blocks/worker-brief` | ✅ |
| run-workers | how a root session runs seats on its host | `blocks/run-workers` (the three adapters point here) | ✅ |
| task-file | the task file and the resume | `blocks/task-file` | ✅ |
| cite-sources | each claim cites its owning source; quote only text read; mark snippets | `blocks/cite-sources` | ✅ |
| link-check | a cited page says the claim | `blocks/link-check` | ✅ |

## Stays whole

These are gates or control loops. A split would let a step claim the gate without its check. A
workflow calls them as one unit.

| Unit | Why |
|---|---|
| implement `pair` mode chunk loop | propose, approve, patch, check, commit is one gate the edit hook reads |
| implement `auto` mode | the hands-off build loop and its stop |
| review check mode core | units, checklist, owner map and verdict are one coverage gate |
| web-search depth gate and fetch ladder | the gate enforces the depth rule; the ladder is a policy of this one owner |
| council protocol | one sequence from Round 0 to cross-examination |
| decisions | capture gate, tiers and immutable body are one contract |
| step-run | status line, pick-up, close and wait are one machine |
| glitch-walk start | its script refuses, stops and gives exact words; it runs as one step |
| engineering-standards tables | each row already has an ID |
| `build` skill seam law | one idea across three files in the `build` skill; it moves with `story` |
| review cleanup mode pass | the six emissions in order are one gate; a step that emitted nothing did not run |
| review walk mode steps | each step needs the reviewer, and the map, comments and walk share one run |
| implement step graph | the step list, status and catalog are one machine that `implement_run.py` runs |

