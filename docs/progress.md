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
| C. yelo retirement | 🔄 code done, Mac not done | main, the owner's Mac |
| D. Web UI port | 🔄 | branches `web-pilot`, `web-port` |
| E. Records and branches | ⬜ | `docs/decisions/`, `wt/` |

Order: A comes first, because a block edit must land in the one kit source. B follows A. C and D
run beside A and B, because they touch other files.

## Decisions

1. Swarm is the one package. It owns the CLI, `swarm serve` and `web/`, the Swift shell, accounts,
   usage, agent profiles, and the kit source in `skills/kit/`.
2. yelo and agent-kit stop getting edits. Each gets a README pointer to swarm and is archived.
3. dotfiles stays private. It keeps `~/.flow/domains.md`, `~/.claude/references`,
   `~/.claude/scripts`, the global `AGENTS.md`, and the owner's skill links.
4. Parts stay separable by modules, not by crates (separate Rust packages). The kit is plain data
   with no Rust dependency. `accounts`, `profiles` and `usage` own identity and quota. `serve` is
   behind a cargo feature and owns no domain rule. Managed edits own every write outside the swarm
   home. A crate split waits until a second user needs one part alone.
5. On the owner's Mac, the CLI skill roots link to this checkout's `skills/kit/skills/*`, so an edit
   is live at once. Other Macs use Setup links to `~/.swarm/skills`.
6. No new block until one block has passed a real run. Each rule has one owner text, and other
   files point to it.
7. A task's workflow is a ledger written once at the start from a menu of blocks, and code runs it
   on the step-run files. A change is a visible re-plan folder that names its parent (council
   2026-10-13 composable-blocks).
8. Review loops stop in code. `review_check.py start` owns a 3-round cap keyed by an explicit
   parent-loop key that fixes and re-plans inherit (K1). The no-progress stop compares open issues,
   not the fix count (P2). Both are owner choices of 2026-10-13.
9. Skills are cut into blocks now, one skill at a time, starting with ink; each cut block gets a
   second user in one live run (owner choice of 2026-10-13). `docs/blocks.md` holds the split map,
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
| A4 | Repoint the owner's links to `~/work/swarm/wt/main/skills/kit/skills/*` in one dotfiles commit; replace the real folders in `~/.claude/skills` with links | ⬜ | `realpath` of each kit name resolves under `skills/kit/`; new Claude, Codex and AGY sessions list the same skills as before |
| A5 | Amend ADR 0066: the owner's Mac reads the checkout, other Macs read the Setup copy | ⬜ | ADR text |
| A6 | Make Setup show skipped skill links as not ready, or say where each one points | ❓ | owner decides if Setup should change |

Note. `~/.flow/domains.md` names `ink/references/story-and-motion.md`, `build.md` and `qa.md` for
the creative-coding domain. On branch `skill-blocks` those files keep only ink's own rules and point
to blocks, so A4 also replaces those lines with `block:` lines.

Note. `~/.claude/skills/deliver` differs from agent-kit only by `scripts/__pycache__`, so no local
skill edit is at risk in A4.

## B. Skills from blocks

A block is a folder `skills/kit/blocks/<id>/` whose `SKILL.md` has Agent Skills frontmatter, then
`Rules`, `Example`, `Check (script)` and `Check (judge)`. `skills/kit/blocks/blocks.toml` owns the format. `flow_run.py start` copies the
blocks a step names into its step file, adds the script checks as todos, and records each block's
hash. The loader is complete and tested. No real run has used a block yet, because
`~/.flow/domains.md` names no `block:` line and no repo has a `.blocks.toml`.

### Blocks that exist (18)

| Block | Concept | Serves | Rule owner today |
|---|---|---|---|
| first-principles | the rules every step keeps (base) | every step | prove-it-works, review-check lens |
| behavior-claims | frame as true-or-false claims with done-when | frame | html-plan pack, pre-mortem |
| decision-with-default | a real fork asked on its claim, with a default | contracts, design | html-plan pack, plan-layout |
| call-tree | impact as a marked call tree with real paths | impact | html-plan pack |
| red-first-check | each behavior's check fails before the change | build | prove-it-works (copy) |
| done-when-evidence | close maps each done-when to evidence | close | flow, prove-it-works |
| tech-diagram-concept | draw only when text cannot hold the shape | design, build | explain-formats |
| html-diagram | diagrams as HTML boxes that reflow | design, build | this block (explain-formats points here) |
| diagram-correctness | each diagram tells the truth and reads on a phone | review | explain-formats, ink qa |
| look-grammar | the look as tokens in `DESIGN.md` | design | this block |
| style-frames-concept | frames from real code, scored beside the reference | design | this block |
| headless-shot | screenshots with chrome-headless-shell | design, build, review | this block |
| story | story sheet, structures, shot list, pacing | design, story | this block (cut from ink) |
| reference-analysis | find, inspect, measure and label references | design | this block (cut from ink) |
| visual-qa | frame and motion checks, the QA log | review | this block (cut from ink) |
| build-site | what changes when the piece is a website | design, build | this block (cut from ink's site.md) |
| build-canvas | the story engine, rendered from time and one seed (pack) | build | this block (cut from ink) |
| web-ui-basics | the floor for a one-page site | build | web guidelines pack, ink |

"(copy)" means the block repeats rules that still live in the named skill. That gives one rule two
texts. B1 fixes this for each block that a run loads.

### Known defects

- `blocks/decision-with-default/SKILL.md:18` says to ask only about real forks, but its judge
  check at `:37` fails a plan with fewer than 2 questions.
- `behavior-claims`, `call-tree`, `done-when-evidence`, `tech-diagram-concept` and
  `diagram-correctness` name flow step files (`01-frame.md`, `04-impact.md`, `07-close.md`,
  `02-design.md`, `06-review.md`), so a second workflow cannot use them. `style-frames-concept`
  and `look-grammar` were fixed on 2026-10-13.
- `references/step_run.py:67-70` drops a `Needs` word that matches no step, with no error.
- `blocks/web-ui-basics/SKILL.md:44-47` checks only `*.html`, `*.css`, `*.js` and `index.html`, so a TSX
  slice gets no script check.
- `blocks/web-ui-basics/SKILL.md:32` says UI motion stays under 300 ms, and transitions.dev tokens use
  400-500 ms. Each motion task names one timing owner.

### Skills: compose or keep whole

| Skill | Plan | Why |
|---|---|---|
| flow | compose (first); writes the ledger for code tasks | the loader is already in `flow_run.py` |
| research | writes the ledger for questions; `web needs local` only when local findings set the query | the owner's local-then-web example |
| motion (pack) | emil pack owns UI motion, ink owns story motion; transitions.dev is link or install only | license forbids copying its library |
| ink | compose (second) by pointer lines to look-grammar, style-frames-concept, headless-shot; delete its copies | the blocks were cut from ink |
| explain-formats (reference) | point to tech-diagram-concept, html-diagram, diagram-correctness | already points to html-diagram |
| pair, deliver, review-check core, decisions, council protocol, step-run, web-search depth gate | keep whole | each is one control loop or gate; a split breaks it |
| orchestrate-claude, -codex, -agy, council, web-search | point to one worker-brief owner section (B12) | the report-only rule has six texts that disagree |

### Steps

Order from the 2026-10-13 composable-blocks council: the review stops, then the ledger checks, then
the pilot, then packs.

| # | Step | State | Check |
|---|---|---|---|
| B1 | Add the 3-round cap to `review_check.py start`, keyed by a parent-loop key in run metadata (K1); fixes and re-plans inherit it; one user `continue:` line allows one more round, once; a refusal keeps the verdict; flow and deliver pass the key and point to the guard | ⬜ | tests: round 4 refused through flow and standalone deliver, also after a fix or re-plan with a new base; two tasks on one base have separate budgets; `continue:` works once; answer-only `verdict` costs no round; APPROVE on the last allowed run closes |
| B2 | Add the no-progress stop by issue identity (P2): `verdict` writes a snapshot of open findings and check results, `start` compares them by file, symbol, rule and source evidence; an uncertain match falls back to the cap | ⬜ | fix A then new B does not stop; the same open issues and results stop; moved lines still match |
| B3 | Make `step_run.py` refuse an unknown or ambiguous `Needs` word, a cycle, or a missing required gate | ⬜ | tests for each refusal |
| B4 | Fix the owner of each block the pilot loads, and the defects above | ⬜ | `rg` finds one text per rule; the flow tests pass |
| B5 | Pilot `web-ui-basics` and `headless-shot` on a static HTML slice (not `web-port`): one `block:` line in the `web-ui` build step of `~/.flow/domains.md` and a `.blocks.toml` | ⬜ | block text in the step file, each block todo closed with evidence, a fresh agent resumes from the files, pair still waits for approval |
| B6 | Judge the pilot against the same slice without blocks; keep or drop | ⬜ | written result with both runs: defects, review rounds, time, loaded text |
| B7 | Add `Keywords:` beside `Signals:` in `domains.md`, matched against the filled frame at `take`; the model confirms, drops or adds a match once, with a reason line | ⬜ | a "transition" task in a non-web-ui repo gets the motion pack; a non-motion task gets none |
| B8 | Motion pack: an index in the emil pack owner, item files, a check, `Keywords:`; ink keeps story motion; web-ui-basics points to it | ⬜ | the step file names one timing owner and an "item files used" todo |
| B9 | Plan block and template format: predefined templates fix required stages and the end; the plan block may add any block, and may build a ledger with no template when none fits (frame, end condition and close still required); one reason line per block | ⬜ | two tasks run by hand, one with a template and one without |
| B10 | Merged into B17 | — | — |
| B11 | Small judge (Jev or Laya) for pack choice: 2+ missed or wrong packs in 10 trial lines starts a benchmark; adopt only on a held-out win with no missed gate | ⬜ | benchmark result |
| B12 | One provider-neutral worker-brief section; the five other texts point to it | ⬜ | `rg` for the report-only rule finds one owner |
| B13 | List block candidates by reuse count; extract only rules with two or more real users | ⬜ | each new block names its two users |
| B14 | Move `resolve_blocks` from `flow_run.py` into one shared kit script when a second skill has a run script that needs it | ⬜ | two run scripts import one resolver |
| B15 | Write the split map: every skill's parts as blocks, rule cards, or units that stay whole | 🔄 draft; ink done, the other skills open | `docs/blocks.md` |
| B16 | Block format v2: one folder per block with Agent Skills frontmatter (`name`, `description`, `license`, `metadata` with kind, owns, serves, needs, gives, conflicts, users); move the 13 blocks; update `blocks.toml` and the `flow_run.py` resolver in the same step | ✅ branch `skill-blocks`, eb7f603f..3553867d | the flow tests pass; each block validates as an Agent Skills `SKILL.md` |
| B17 | Cut ink: reference-analysis, story, visual-qa, build-site, build-canvas; one owner for look-grammar, style-frames, headless-shot; ink keeps its contract and its routes as templates | ✅ branch `skill-blocks`, da3b8566..5213c714 | ink's references hold no rule that a block holds; an ink `prototype` run still passes |
| B18 | Second user: a glitch-walk page uses story, look-grammar, headless-shot and visual-qa | ⬜ | one live run, each block todo closed with evidence |
| B19 | serve-local and annotate-loop (Plannotator, strict): view-and-talk is required in the `walk` template; a missing `plannotator` sets `unavailable plannotator` and stops; each round writes `annotate/<round>.json`; the loop uses the B1 and B2 stop | ⬜ | one glitch-walk run where an owner comment reaches the agent and the page is rebuilt; a run without the binary stops with the status |
| B20 | Package the kit as a skills-only Agent Plugins 1.0 package (`plugin.json` at `skills/kit/`); Codex reads it as specified, Claude Code and AGY load its skills (`docs/blocks.md`, Packaging) | ⬜ | `codex plugin add`, `claude plugin details`, `agy plugin validate` each list the kit's skills |
| B21 | Install `plannotator` on the PATH on the owner's Mac from a checked release | ❓ | owner approves the install; `command -v plannotator` |

Rejected for now: `minimal-diff`, `verification-evidence`, `trust-boundary`, `reversibility`. Each
repeats a rule in `first-principles.md`, `done-when-evidence.md`, `red-first-check.md` or the global
`AGENTS.md`.

Trial evidence (2026-10-09, two rounds, small samples): Sonnet gained from blocks, Codex gained only
with the merged profile, and blocks cut rule text by 50-60 %. Copied-in rules beat pointer lines in
an earlier test, so B10 and B12 must show the agent read the pointed file.

## C. yelo retirement

Code is done. Main has no `yelo` reference in `src/` or `ui/` (PR #54). On the owner's Mac, yelo
0.10.0, the `io.github.priyanshuupadhyay.yelo-hud` LaunchAgent and `UsageHUD.app` are still
installed. ADR 0071 keeps yelo profile folders and Keychain entries by reference, so no account
data moves.

| # | Step | State | Check |
|---|---|---|---|
| C1 | Test on the installed release: named and auto account launch, shared Claude chats, Codex resume with an explicit account, usage failure states, HUD panel | ⬜ | each item seen working |
| C2 | Write down that yelo's Codex session-owner lookup is lost (deferred by the 2026-10-12 accounts council) | ⬜ | a line in ADR 0071 or this file |
| C3 | Remove the yelo zsh integration from dotfiles before the binary | ⬜ | a new shell starts `claude` and `codex` with no yelo call |
| C4 | Unload the HUD LaunchAgent, remove `UsageHUD.app`, `brew uninstall yelo` | ⬜ | `command -v yelo` is empty; no yelo LaunchAgent |
| C5 | Deprecate the tap formula with a pointer; add a README pointer and archive the yelo repo | ⬜ | tap and repo state |

## D. Web UI port (ADR 0075)

The UI becomes a React 19 web app on `swarm serve` in a thin Swift WKWebView shell. Swift features
are frozen; Swift gets bug fixes only until the web app reaches parity and `ui/` (except the shell)
is deleted.

| # | Step | State | Where |
|---|---|---|---|
| D1 | `swarm serve`, web pilot, budget suite, SwarmWeb shell, ADR 0075 | ✅ on branch, not merged | `web-pilot` (7 commits ahead of main) |
| D2 | shadcn/ui frame, Rust transcript rows, chat action routes, composer | 🔄 active worker, uncommitted files | `web-port` (12 commits ahead, on `web-pilot`) |
| D3 | Account and skill routes in `serve` through the Rust owners | ⬜ | `serve.rs` has no such routes yet |
| D4 | Parity, then delete `ui/` except the shell | ⬜ | speed budgets in ADR 0075 still pass |

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
