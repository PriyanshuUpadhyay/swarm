---
status: accepted
date: 2026-10-24
deciders: [user]
related: ["0011", "0040", "0041", "0044"]
informed-by:
  - "Council 2026-10-23 (GPT, Gemini, Claude seats), GO-WITH-CHANGES; log in the owner's ~/dotfiles/docs/council/2026-10-23-project-board.md"
  - "owner, 2026-10-23, on S6a: option 1, no owner-approval command; the lead relays a chat answer as --relayed"
  - "Phase 0 baseline of 5 swarm runs: 9 of 41 waste items were ones a shared board could catch; ~/.claude/reports/2026-10-23-project-board-plan/baseline.md"
  - "DeLM, arXiv 2606.10662: shared context and a task queue with no main agent; supports claims and findings, but its baselines have no checker"
  - "arXiv 2607.09493: selective memory 12/12, full history 8/12, no memory 0/12"
links:
  - "Plan: ~/.claude/reports/2026-10-23-project-board-plan/plan.md"
  - "migrations/0015.sql, src/store/board*.rs, src/cli/board.rs, src/guard.rs board_wall"
---

# 0124. A project board in the binary: typed posts, fenced claims, and walls that swarm enforces

## Context and Problem Statement

Agents in one project found the same facts again in each run, worked from wrong guesses, and
waited on each other with no record. `swarm send` moves text between seats of one session, and it
keeps nothing that a later run can find. A skill can ask agents to share notes, but a skill rule
is a request, and an agent can skip it.

## Considered Options

- A board in the binary: typed append-only posts and claims per project, with each rule a
  refusal (chosen).
- A shared notes file per project that a skill tells agents to keep.
- No board: the lead keeps the state in its own chat and relays it with `swarm send`.

## Decision Outcome

Chosen: a board in the binary, because a refusal holds when an agent ignores a skill, and a post
with a source can be checked when the file changes.

- **One board per project.** The id is `repository:<git common dir>`, else `folder:<path>`, the
  same id as the project list (ADR 0011), so all worktrees of a repo share one board. A folder
  in the temp dir outside git gets no board. `swarm board relink <old-id>` copies a moved
  repo's board into an empty board; only the lead runs it.
- **Three commands.** `swarm board post <ask|finding|decision|claim|handoff>`, `swarm board
  answer B-n`, and `swarm board read [B-n]`. The body comes on stdin. Posts show as `B-n`.
- **Posts are append-only.** Triggers refuse UPDATE and DELETE. A change is a new post with
  `--supersedes B-n`, and only the newest post in a chain can be superseded.
- **A finding, decision, or answer names a source**, `path@blob` or a URL. A file source must
  match the file now, and `board read` marks the post stale when the file changes.
- **Claims carry a generation fence.** A claim takes a work key for a lease. A renewal names the
  current generation and writes no post. A new holder after the lease ends gets generation + 1.
  An old holder's handoff with a stale generation is refused. A 4th holder in one session is
  refused, and the work parks for the owner; a later session starts its tries again. `swarm close`
  releases the closed seat's live claims, so the lead's relaunch claims at once (probe c3, 2026-10-26).
- **Pings are fixed text.** An addressed ask, an answer to the asker, and an addressed handoff
  add one `message` row from the paneless `board` sender, in the post's transaction, for example
  "board post B-7: ask from c1. Run swarm board read B-7." Agent text never goes in a ping.
  Board rows skip the talk-mode route, so lane mode does not block them; `swarm send` keeps its
  lane refusal. A finding, a claim, and an unaddressed handoff ring nobody.
- **Ask chains stop at depth 3.** A deeper ask is stored, rings no peer, and rings the lead once.
- **No owner approval in v1.** `--needs-owner` parks a decision, rings the lead, and writes one
  notice. Only the lead posts `--relayed`, and `read` shows it as "relayed by lead".
- **The checker has a wall.** A worker's `board read` leaves out posts by role `checker` in its
  own session, except a post sent to that worker and an answer to its own ask. A checker post
  from another session stays readable, so a later run gets an earlier run's findings. A worker hands off a work key only
  after a checker finding with that key exists. `swarm guard` denies a worker's tool call that
  names `.swarm/boards`. The wall also works the other way: a checker may not list the board,
  and reads only `B-n`, the posts its brief names (probe d2, 2026-10-26).

### Consequences

- Good: a fact, a claim, or a parked decision outlives the session and is found by the next run.
- Good: each rule lives in one place, the binary, and its refusal names the fix.
- Bad: a session with no checker seat cannot hand off a worker's claim; the lease must end, or
  the lead takes the work.
- Bad: a new table and a hidden `board` agent row in each session that gets a ping.

## Decision Drivers

- A rule that an agent can skip is not a rule (ADR 0040, ADR 0044).
- Split agents by what each may see or touch, not by job title.
- Keep agent kinds few: the lead, workers, and the checker.

## Pros and Cons of the Options

### A board in the binary
- Good, because each rule is a refusal that a test covers.
- Bad, because the protocol is a public interface that later changes must keep.

### A shared notes file per project
- Good, because it needs no code.
- Bad, because nothing checks a source, stops two agents on one task, or keeps the checker's
  tests from workers.

### No board
- Good, because nothing changes.
- Bad, because the baseline found 9 of 41 waste items that a shared record could catch.

## Confirmation

The store tests have one test per refusal, a race of two claims, and the ping and wall rules. The
trial in step 18 of the plan keeps the board when the board-catchable count falls to 1 or less per
run, 1 run in 3 reuses an older post, and no drop rule fires.

## Known limits

- **Direct file reads.** The wall is best effort. A worker that builds the path of a body file
  at run time can read it, because the guard sees only the text of a tool call.
- **Forged identity.** The caller is named by `SWARM_SESSION_ID` and `SWARM_AGENT_ID`, which a
  seat can change.
- **No owner proof.** No swarm channel can prove that the owner acted, so v1 has no owner
  approval command.

## Links

- Related: 0011 (project id), 0040 (`swarm guard`), 0041 (rings), 0044 (only the chair notifies).
