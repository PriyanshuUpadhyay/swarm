# Progress

This file tracks the work that makes swarm the one package for accounts, skills and the UI. It
comes from the council of 2026-10-13, which gave a unanimous GO-WITH-CHANGES on one swarm package.
Update a row when its state changes, and name the commit, branch or PR that proves it.

Status symbols: ✅ done, 🔄 in progress, ⬜ not started, ⛔ blocked, ❓ waits for an owner decision.

Last checked: 2026-10-13, on main at `dabc9ef8`. Installed swarm is 0.12.0.

## Now

| Track | State | Where |
|---|---|---|
| A. Skills setup (one kit source) | ⬜ next | main, dotfiles |
| B. Skills from blocks | ⬜ after A1-A4 | main `skills/kit/` |
| C. yelo retirement | 🔄 yelo removed and archived; C1 has open items | main, the owner's Mac |
| D. Web UI port | 🔄 merged, one budget miss | main |
| E. Records and branches | ⬜ | `docs/decisions/`, `wt/` |

Order: A comes first, because a block edit must land in the one kit source. B follows A. C and D
run beside A and B, because they touch other files.

## Decisions

1. Swarm is the one package. It owns the CLI, `swarm serve` and `web/`, the Swift shell, accounts,
   usage, agent profiles, and the kit source in `skills/kit/`.
2. yelo and agent-kit stop getting edits. Each gets a README pointer to swarm and is archived.
3. dotfiles stays private. It keeps `~/.flow/domains.md`, `~/.claude/references`,
   `~/.claude/scripts`, the global `AGENTS.md`, and the owner's links to skills outside the kit plugin.
4. Parts stay separable by modules, not by crates (separate Rust packages). The kit is plain data
   with no Rust dependency. `accounts`, `profiles` and `usage` own identity and quota. `serve` is
   behind a cargo feature and owns no domain rule. Managed edits own every write outside the swarm
   home. A crate split waits until a second user needs one part alone.
5. Each CLI installs the kit as the plugin `swarm`, because a skill reaches its blocks two folders
   up from its own folder, and only a copy of the whole kit keeps that path (`docs/blocks.md`,
   Packaging). On the owner's Mac, Claude Code reads the checkout, because its marketplace is this
   folder; Codex and AGY read an installed copy, so the owner updates the plugin after a kit change.
6. No new block until one block has passed a real run. Each rule has one owner text, and other
   files point to it.
7. A task's workflow is a ledger written once at the start from a menu of blocks, and code runs it
   on the step-run files. A change is a visible re-plan folder that names its parent (council
   2026-10-13 composable-blocks).
8. Review loops stop in code. `review_check.py start` owns a 3-round cap keyed by an explicit
   parent-loop key that fixes and re-plans inherit (K1). The no-progress stop compares open issues,
   not the fix count (P2). Both are owner choices of 2026-10-13.
9. Skills are cut into blocks now, one skill at a time, starting with build; each cut block gets a
   second user in one live run (owner choice of 2026-10-13). Later the same day the owner asked to
   split every other skill too, with names and descriptions a small model can use, before the merge
   and one trial run. `docs/blocks.md` holds the split map,
   the model (template, plan block, ledger, end condition) and the block format.

## A. Skills setup (one kit source)

Today three kit copies exist. `~/work/agent-kit` is the live one. `skills/kit/` here is one commit
behind (`cc095d3`, agent-kit is at `3b57571`). `~/.swarm/skills/kit` is linked from no CLI root.
Setup reports Skills ready, because it skips owner entries (ADR 0062 amendment,
`src/main.rs:1594-1600`).

| # | Step | State | Check |
|---|---|---|---|
| A1 | Make a read-only matrix of every bundled skill × `.claude/skills`, `.agents/skills`, `.gemini/config/skills`: real target, revision, local differences, proposed link | ⬜ | each name has one future source, and each difference has a preserve-or-merge row |
| A2 | Pull agent-kit `3b57571` into `skills/kit/` on a branch | ⬜ | `diff -r` of agent-kit `skills/` and `skills/kit/skills/` is empty |
| A3 | Stop edits in agent-kit; add its README pointer and archive it | ⬜ | no agent-kit commit after the pull |
| A4 | On the owner's Mac, install the plugin `swarm` from this checkout in Claude Code, Codex and AGY. Remove every kit skill name from `~/.claude/skills`, `~/.agents/skills`, `~/.gemini/config/skills` and their dotfiles sources (`~/dotfiles/home/.claude/skills`, `~/dotfiles/skills/runtimes`), links and real folders alike, in one dotfiles commit, and drop the kit skills from `~/dotfiles/skill-projections.json` and its test `tests/agent-kit/test_kit_links.py`, so `skill-sync` does not make the links again; keep swarm-orchestrator, swarm-voice and the three vendor skills linked. Repoint each absolute kit path to the checkout's `skills/kit/` in every dotfiles file that the check below finds (among them `~/.flow/domains.md`, the global `AGENTS.md`, `fable-orchestrator-mode.md` and `.prime/agent/skills/orchestrate-prime/SKILL.md`); remove the kit copies in `~/.claude/references`. Make `src/host.rs` name the runtime adapter as a skill (`orchestrate-claude`) in place of a skills-folder path. Switch the `any` build and close lines of `~/.flow/domains.md` to `block:prove-it-works` | ⬜ | `claude plugin details swarm`, `codex plugin list`, `agy plugin list` list the kit skills; no skills folder holds a kit name; `rg --hidden -g '!harness-ledger.md' -e '\.claude/skills/' -e '\.claude/references/' ~/dotfiles/home` finds no kit name (the ledger is a record of past runs); a new session in each CLI reads a block; after a kit edit, installing again in Codex and AGY puts the edit in a new session (a scratch test on 2026-10-13 showed that both copy the same version again) |
| A5 | Amend ADR 0066: each Mac reads an installed plugin copy, and the owner updates it after a kit change | ⬜ | ADR text |
| A6 | Make Setup show skipped skill links as not ready, or say where each one points | ❓ | owner decides if Setup should change |
| A7 | Make Setup install the plugin `swarm` from the bundled kit in each CLI, in place of kit skill links, because a link to one kit skill folder cannot reach `blocks/`; Setup keeps the links for swarm-orchestrator, swarm-voice and the three vendor skills, and the root README's Setup step says so | 🔄 `swarm setup plugin` (ADR 0090); ships in the next release | on a Setup-only Mac, each CLI lists the plugin and a session reads a block |

Note. On 2026-10-13 the creative-coding lines of `~/.flow/domains.md` gained `block:` items
(reference-analysis, design-tokens, story, build-canvas, style-frames, visual-qa). Only the `implement`
skill on branch `skill-blocks` knows these blocks, so a creative-coding run with the live agent-kit
copy of the `implement` skill stops with "unknown block id" until `skill-blocks` is merged to main
and A4 installs the plugin. The `any` lines keep the `prove-it-works` skill path until A4, because a
`block:` item there would stop every live run of the `implement` skill. A repo with no `.blocks.toml`
runs the blocks without the base block; the creative-coding blocks also need `site` and `shots`
there. `implement_run.py start` always reads the shipped `blocks.toml` to list chosen and rejected
blocks, so it needs Python 3.11 or later (`/usr/bin/python3` is 3.9).

Note. `~/.claude/skills/deliver` differs from agent-kit only by `scripts/__pycache__`, so no local
skill edit is at risk in A4.

## B. Skills from blocks

A block is a folder `skills/kit/blocks/<id>/` whose `SKILL.md` has Agent Skills frontmatter, then
`Rules`, `Example`, `Check (script)` and `Check (judge)`. `skills/kit/blocks/blocks.toml` owns the format. `implement_run.py start` copies the
blocks a step names into its step file, adds the script checks as todos, and records each block's
hash. The loader is complete and tested. `~/.flow/domains.md` names blocks since 2026-10-13 (see
the note in track A). No real run has used a block yet. `docs/blocks.md` maps every kit skill to its
blocks.

### Blocks that exist (42)

Each block owns its rule text; the skill that held it points to the block.

| Block | Concept | Serves |
|---|---|---|
| base-rules | the rules every step keeps (base) | every step |
| requirements | frame as true-or-false claims with done-when (was behavior-claims) | frame |
| decision-with-default | a real fork asked with a default, and the trade-off form | contracts, design |
| call-tree | impact as a marked call tree with real paths | impact |
| prove-it-works | what proves a change works (was the skill's text and red-first-check) | build, review, close |
| checked-units | small units that each end in a check (was sequence-verifiable-units) | build |
| when-to-extract | code stays inline without a reason (was minimize-reader-load) | build, review |
| done-when-evidence | close maps each done-when to evidence | close |
| when-to-diagram | diagram size, labels, and the design-step record | design, build |
| html-diagram | diagrams as HTML boxes that reflow | design, build |
| diagram-check | each diagram tells the truth and reads on a phone | review |
| design-tokens | the look as tokens in `DESIGN.md` | design |
| look-check | frames from real code, scored beside the reference | design |
| headless-shot | screenshots with chrome-headless-shell | design, build, review |
| story | story sheet, structures, shot list, pacing (cut from build) | design, story |
| reference-analysis | find, inspect, measure and label references (cut from build) | design |
| visual-qa | frame and motion checks, the QA log (cut from build) | review |
| build-site | what changes when the piece is a website (cut from build) | design, build |
| build-canvas | the story engine, rendered from time and one seed (pack, cut from build) | build |
| web-ui-basics | the floor for a one-page site | build |
| worker-brief | what a worker brief holds and how a worker acts (B12) | every step that sends work |
| run-workers | how a root session runs seats on its host (was the adapters' shared text) | every step that uses seats |
| task-file | the task file and the resume (was in pair and deliver) | build |
| local-research | what local sources say (was research 02-local) | research, contracts |
| deep-search | search past the known answers (was research and web-search depth text) | research, web search |
| cite-sources | each claim cites its owning source | research, web search |
| link-check | every cited link opens and says the claim | report, merge |
| report | the shape of a research report and its reply | report, close |
| tool-trial | try three candidates on the same task (was research 04-test) | research, contracts |
| planner | which blocks a task uses, in which order, with which inputs | plan |
| plan-chart | the check and the HTML chart of a plan's step graph | plan |
| plan-review | the owner's approval of the current plan, then the start | plan |
| comment-round | one Plannotator round with file hashes, answers and the loop stop | plan, view and talk |
| page-comments | the comment, fix and check loop for a built page | view and talk |
| serve-local | a loopback view of the checked build | view and talk |
| change-review | the full review-check verdict on a plan's commits | review |
| scratch-home | trial and test state apart from the live home | build, review, research |
| ui-motion | UI timing and easing from one timing owner | build, review |
| voice-reply | the final reply, by the session's reply rules | close |
| voice-teach | an explanation in learning order | close |
| voice-review | review comments in the reviewer's voice | review, close |
| voice-pr | a PR title and body draft | close |

### Known defects

- `references/step_run.py:67-70` drops a `Needs` word that matches no step, with no error.
- `blocks/web-ui-basics/SKILL.md:44-47` checks only `*.html`, `*.css`, `*.js` and `index.html`, so a TSX
  slice gets no script check.
- `blocks/ui-motion/SKILL.md` delegates timing to one owner. The emil pack still has a
  200-500 ms modal row and an under-300 ms line; that vendor conflict needs a scoped choice when used.
- A repo with no `.blocks.toml` gets the base `Off:` line in all seven step files, also in steps
  that name no block.

### Skills: compose or keep whole

`docs/blocks.md` ("Every skill, split") maps each skill to the blocks it points to and the part
that stays whole. Each stays-whole part is one control loop or gate, and a split would break it.

### Steps

Order from the 2026-10-13 composable-blocks council: the review stops, then the ledger checks, then
the pilot, then packs.

| # | Step | State | Check |
|---|---|---|---|
| B1 | Add the 3-round cap to `review_check.py start`, keyed by a parent-loop key in run metadata (K1); fixes and re-plans inherit it; one user `continue:` line allows one more round, once; a refusal keeps the verdict; implement and deliver pass the key and point to the guard | ⬜ | tests: round 4 refused through implement and standalone deliver, also after a fix or re-plan with a new base; two tasks on one base have separate budgets; `continue:` works once; answer-only `verdict` costs no round; APPROVE on the last allowed run closes |
| B2 | Add the no-progress stop by issue identity (P2): `verdict` writes a snapshot of open findings and check results, `start` compares them by file, symbol, rule and source evidence; an uncertain match falls back to the cap | ⬜ | fix A then new B does not stop; the same open issues and results stop; moved lines still match |
| B3 | Make `step_run.py` refuse an unknown or ambiguous `Needs` word, a cycle, or a missing required gate | ⬜ | tests for each refusal |
| B4 | Fix the owner of each block the pilot loads, and the defects above | 🔄 red-first-check merged into prove-it-works, the decision-with-default defect fixed (branch `skill-blocks`) | `rg` finds one text per rule; the implement tests pass |
| B5 | Pilot `web-ui-basics` and `headless-shot` on a static HTML slice (not `web-port`): one `block:` line in the `web-ui` build step of `~/.flow/domains.md` and a `.blocks.toml` | ⬜ | block text in the step file, each block todo closed with evidence, a fresh agent resumes from the files, pair still waits for approval |
| B6 | Judge the pilot against the same slice without blocks; keep or drop | ⬜ | written result with both runs: defects, review rounds, time, loaded text |
| B7 | Add `Keywords:` beside `Signals:` in `domains.md`, matched against the filled frame at `take`; the model confirms, drops or adds a match once, with a reason line | ⬜ | a "transition" task in a non-web-ui repo gets the motion pack; a non-motion task gets none |
| B8 | Motion pack: an index in the emil pack owner, item files, a check, `Keywords:`; build keeps story motion; web-ui-basics points to it | 🔄 basic `ui-motion` names one timing owner, and web-ui-basics and build-site point to it (branch `plan-blocks`); no item files or `Keywords:` yet | the step file names one timing owner and an "item files used" todo |
| B9 | Plan block and template format: predefined templates fix required stages and the end; the plan block may add any block, and may build a ledger with no template when none fits (frame, end condition and close still required); one reason line per block | 🔄 basic blocks `planner`, `plan-chart`, `plan-review`, `voice-reply`, `voice-review` and the skill `plan`, branch `plan-blocks` (2026-10-13); the 2026-10-13 block-rules council added 8 blocks, renamed behavior-claims to requirements, and bound plan approval to file hashes; no templates yet, because the owner wants blocks picked per task | two tasks run by hand, one with a template and one without |
| B10 | Merged into B17 | — | — |
| B11 | Small judge (Jev or Laya) for pack choice: 2+ missed or wrong packs in 10 trial lines starts a benchmark; adopt only on a held-out win with no missed gate | ⬜ | benchmark result |
| B12 | One provider-neutral worker-brief section; the five other texts point to it | ✅ `blocks/worker-brief`, branch `skill-blocks` | `rg` for the report-only rule finds one owner |
| B13 | List block candidates by reuse count; extract only rules with two or more real users | ⬜ | each new block names its two users |
| B14 | Move `resolve_blocks` from `implement_run.py` into one shared kit script when a second skill has a run script that needs it | ⬜ | two run scripts import one resolver |
| B15 | Write the split map: every skill's parts as blocks, rule cards, or units that stay whole | ✅ every skill split, branch `skill-blocks` | `docs/blocks.md` |
| B16 | Block format v2: one folder per block with Agent Skills frontmatter (`name`, `description`, `license`, `metadata` with kind, owns, serves, needs, gives, conflicts, users); move the 13 blocks; update `blocks.toml` and the `implement_run.py` resolver in the same step | ✅ branch `skill-blocks`, eb7f603f..3553867d | the implement tests pass; each block validates as an Agent Skills `SKILL.md` |
| B17 | Cut build: reference-analysis, story, visual-qa, build-site, build-canvas; one owner for design-tokens, style-frames, headless-shot; build keeps its contract and its routes as templates | ✅ cut done, branch `skill-blocks`, da3b8566..5213c714; the live run is B18 | build's references hold no rule that a block holds; a build `prototype` run still passes |
| B18 | Second user: a glitch-walk page uses story, design-tokens, headless-shot and visual-qa | ⬜ | one live run, each block todo closed with evidence |
| B19 | serve-local and annotate-loop (Plannotator, strict): view-and-talk is required in the `walk` template; a missing `plannotator` sets `unavailable plannotator` and stops; each round writes `annotate/<round>.json`; the loop uses the B1 and B2 stop | 🔄 basic `serve-local`, `comment-round` (exit 2 with `unavailable plannotator`; round 4 refused in `comment_round.py` until B1 and B2 exist) and `page-comments`, branch `plan-blocks`; no live round yet | one glitch-walk run where an owner comment reaches the agent and the page is rebuilt; a run without the binary stops with the status |
| B20 | Package the kit as a skills-only Agent Plugins 1.0 package (`plugin.json` at `skills/kit/`); Codex reads it as specified, Claude Code and AGY load its skills (`docs/blocks.md`, Packaging) | ✅ manifests and marketplace on branch `skill-blocks`; install is A4 and A7 | a probe package installed in each CLI, and a model in each read a block by `../../blocks/` |
| B21 | Install `plannotator` on the PATH on the owner's Mac from a checked release | ✅ 0.28.17 in `~/.local/bin`, SHA-256 and `gh attestation verify` passed (2026-10-13) | owner approves the install; `command -v plannotator` |
| B22 | Parent blocks, slice 1: the diagram family; flat folders, one `metadata.parts` owner, shared rules loaded once before selected parts in implement; the plan chart refuses parent ids until B14 | 🔄 branch `block-parents-diagram` (2026-10-16); start proof passes; live run with closed todos is open. The form choice is recorded in `docs/blocks.md`, Ownership and conflicts. C6 (375 px checks in `html-diagram` and `diagram-check`) and C7 (Real over drawn in `base-rules` and `diagram-check`) remain open. Pointer-only routes through `web-ui-basics` or `explain-formats.md` may miss the parent rules (C5, accepted cost). | implement, plan-chart, and step-run scripts print `ok`; membership and copied-sentence tests pass; run `python3 -I skills/kit/skills/implement/scripts/implement_run.py start` (use the script's absolute path from the scratch repo) in a scratch repo with design `block:when-to-diagram` and build `block:diagram` in its domains file, then count `===== block:diagram =====` in each generated step (expected 1) and its three judge checks in `06-review.md` (expected 1 each); no model behavior or full HTML pilot claim |

Rejected for now: `minimal-diff`, `verification-evidence`, `trust-boundary`, `reversibility`. Each
repeats a rule in the `base-rules`, `done-when-evidence` or `prove-it-works` block, or the global
`AGENTS.md`.

Trial evidence (2026-10-09, two rounds, small samples): Sonnet gained from blocks, Codex gained only
with the merged profile, and blocks cut rule text by 50-60 %. Copied-in rules beat pointer lines in
an earlier test, so B10 and B12 must show the agent read the pointed file.

## C. yelo retirement

Main has no `yelo` reference in `src/` or `ui/` (PR #54). On 2026-10-14 the shell wrappers moved
to `swarm accounts pick` (ADR 0081, release 0.0.2), and yelo was removed from the owner's Mac and
archived. ADR 0071 keeps yelo profile folders and Keychain entries by reference, so no account
data moves. The removed files are in the owner's Trash.

| # | Step | State | Check |
|---|---|---|---|
| C1 | Test on the installed release: named and auto account launch, shared Claude chats, Codex resume with an explicit account, usage failure states, HUD panel | 🔄 | 0.0.2 in real panes: auto `claude` (`acct sirsendu`), `pi --profile abhi`, `codex resume --last` in the session's home. Not seen: shared Claude chats across accounts, usage failure states, the usage panel |
| C2 | yelo's Codex session-owner lookup | ✅ | ported, not lost: `accounts pick --session ID` and `--last [--all]` (ADR 0081, `a6609421`) |
| C3 | Remove the yelo zsh integration from dotfiles before the binary | ✅ | dotfiles `7207d2c`; a new shell has `claude`, `codex` and `pi` functions and no `_yelo_profiles` |
| C4 | Unload the HUD LaunchAgent, remove `UsageHUD.app`, `brew uninstall yelo` | ✅ | `command -v yelo` is empty; `launchctl print` finds no yelo service |
| C5 | Remove the tap formula and archive the yelo repo | ✅ | tap `66be8a0` deletes `Formula/yelo.rb` and points the README at swarm; `PriyanshuUpadhyay/yelo` is archived with no README pointer |

## D. Web UI port (ADR 0075)

The UI becomes a React 19 web app on `swarm serve` in a thin Swift WKWebView shell. Swift features
are frozen; Swift gets bug fixes only until the web app reaches parity and `ui/` (except the shell)
is deleted.

| # | Step | State | Where |
|---|---|---|---|
| D1 | `swarm serve`, web pilot, budget suite, SwarmWeb shell, ADR 0075 | ✅ | merged in 759e3bb5 through `web-port` |
| D2 | shadcn/ui frame, Rust transcript rows, chat action routes, composer | ✅ | merged in 759e3bb5 (S1-S7) |
| D3 | Account and skill routes in `serve` through the Rust owners | ✅ accounts; skills dropped | `/api/accounts*` in `serve.rs`; ADR 0076 amendment takes the Skills editor out of the web UI |
| D4 | Parity, then delete `ui/` except the shell | 🔄 | `ui/Sources` holds only SwarmWeb and SwarmNotify (58beca2a). `e2e/changes.spec.ts:80` misses its 150 ms budget by about 1 ms in chromium and webkit (debug `swarm`, 2026-10-14) |

## E. Records and branches

| # | Step | State |
|---|---|---|
| E1 | Accept or amend the proposed ADRs that are merged in code: 0062, 0063, 0065, 0066, 0071, 0072, 0073, 0074 | ❓ owner |
| E2 | Close or rebase stale branches: `codex-git-write` (1 ahead, 841 behind), `redesign-settings` (1 ahead, 370 behind) | ❓ owner |
| E3 | Remove the 41 merged worktrees under `wt/` | ⬜ |

## Sources

- Council log: `~/dotfiles/docs/council/2026-10-13-swarm-unify-and-blocks.md`
- Earlier councils: `2026-09-27-public-repo-split`, `2026-10-09-skill-building-blocks`,
  `2026-10-12-swarm-web-ui`, `2026-10-12-swarm-owns-accounts-isolation` (same folder)
- Blocks trial: `~/.claude/reports/2026-10-09-blocks-trial.md`
- Block split map: `docs/blocks.md`
- Council log `2026-10-13-composable-blocks` and report `~/.claude/reports/2026-10-13-composable-skill-blocks.md`
- ADRs 0031, 0062, 0063, 0066, 0071, 0073, 0075 in `docs/decisions/`
