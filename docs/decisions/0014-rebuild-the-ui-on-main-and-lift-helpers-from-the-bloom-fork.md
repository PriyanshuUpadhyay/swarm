---
status: accepted
date: 2026-09-22
deciders: [user]
supersedes: 0001
superseded-by:
related: [0002, 0003, 0013]
informed-by:
  - docs/council/2026-09-22-rebuild-on-main.md
  - docs/council/2026-09-17-swarm-ui-approach.md
links:
  - branch ui-rebuild, from main aba6ee60
  - branch ui-bloom-temp at 23453b1a (the quarry)
---

# 0014. Rebuild the UI on main and lift helpers from the Bloom fork

## Context and Problem Statement

Six days after adopting spatie/bloom (0001), the fork on `ui-bloom-temp` had 120 commits, five
council runs, and a rebuilt centre column, and the owner judged it "not the same product anymore".
The target the owner needs now is the swarm chat, the live pane and the project > worktree >
sessions tree. That is about 25 swarm-specific files (6k lines) plus 2.7k lines of tmux and
SwiftTerm helpers inside a 231k-line Swift tree that otherwise serves features outside the target.
A dark period is acceptable, and the two-day trial the 2026-09-17 council gated on was never run.

## Considered Options

- A. Keep and cut: stay on the fork and delete Bloom subsystems by their callers.
- B. Rebuild on main: a new Swift app in the swarm repo, lifting only the helpers the target needs.
- C. New shell, kept core: new views over the whole SwarmCore library.

## Decision Outcome

Chosen: B, because the rule that decides what the tree shows needs a Bloom `Store` row, so C
would keep the base one layer down, and none of the cut items the 09-17 council required had
landed in five days under A. Extraction is by dependency audit per file, the renderer is cut at
`TranscriptEvent`, the data model comes from the bus (project = repository path, worktree =
`git worktree list`, session = a bus row matched by cwd), `ui/` on `ui-bloom-temp` stays as a
quarry until the gate passes, and every lifted Bloom-origin file keeps the MIT notice.

### Consequences

- Good: every decision in the UI is ours, and the tree is a few thousand lines the compiler names.
- Good: no Bloom Store, migrations, deliveries, runners or JSON-mode agent code to work around.
- Bad: 8 to 12 working days with no usable app, and the diff viewer, PR actions, notes, browser
  and presets that worked on the fork are gone until asked for again.
- Bad: the lifted files are less separable than their names suggest; a lift that becomes a rewrite
  doubles the estimate.

## Decision Drivers

- The owner wants to own the decisions, not work around a base that is nowhere near the target.
- The target narrowed to chat, pane and sessions tree only.
- A dark period is fine; the fork is not the daily tool.

## Pros and Cons of the Options

### A. Keep and cut
- Good, because today's feature level costs zero days.
- Bad, because the cut stalled: the runner, queue, browser and `TranscriptModel` were all still
  present, and both swarm chat views still built `TranscriptModel`.

### B. Rebuild on main
- Good, because the target is about 5% of the tree and already separable.
- Bad, because the gate (the 09-21 smoke list and the 09-17 trial) must be driven through the real
  pane, and pane-lifecycle faults cost days in any shell.

### C. New shell, kept core
- Good, because the shell alone is 5 to 7 days.
- Bad, because the core keeps `Store`, `SessionID` and `WorkspaceID`, so the same open-ended cut
  follows one layer down.

## Confirmation

The decision holds when the gate passes on the real app: one app-started chat and one CLI-started
session in one tree, Escape reaching only the selected pane, quit and reopen with both rows live
and readable. Revisit if the owner puts the diff viewer or PR actions back into the target within
weeks, because then lifting Bloom's tested code makes C cheaper.

## Informed by

- The council log `docs/council/2026-09-22-rebuild-on-main.md` (unanimous GO-WITH-CHANGES, B).
- The prior council `docs/council/2026-09-17-swarm-ui-approach.md` (keep-and-cut, trial never run).

## Links

- Supersedes: 0001. Related: 0002 (the UI still lives in `ui/` of this repo), 0003, 0013 (their
  substance carries over: each agent is a chat and a live pane, every chat is a swarm session).
