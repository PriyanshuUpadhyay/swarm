---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0028", "0039"]
informed-by:
  - "Owner request 2026-10-05: chat readability while keeping everything reviewable"
  - "Owner answers 2026-10-05 (accept all recommendations): Q1 A live fold of 2+ steps, a failed step opens its fold; Q2 A parser kind swarm_ring; Q3 A Show Source shows the translated event JSON; Q4 A fold state in memory per open chat; a failed Codex exec shows as failed"
  - "Owner answer 2026-10-05 (review round 2): \"Lazy, I check the window\": an open fold's steps and the Show Source blocks draw lazily"
  - "Flow folder tmp/flow/2026-10-05-chat-readability (02-design, 03-contracts)"
  - "Codex chair log 2026-09-29 (council test): rings arrive mid-turn as user messages"
---

# 0047. Tool steps between prose fold live, and swarm rings are system rows

## Context and Problem Statement

A chair chat showed each swarm ring as the owner's bubble, Codex exec calls as raw JavaScript, and
runs of tool rows between short prose lines. The fold from ADR 0039 never fired on it: a mid-turn
ring split the turn, and rows seen while the turn ran were pinned open. A failed Codex exec also
showed as finished.

## Considered Options

- Keep 0039's fold (3+ finished tools after the turn ends, rows on screen never fold) and only tag rings.
- Fold every run of 2+ steps between prose, live, with failed steps inside an open fold.
- Fold live, but a failed step breaks the run and stays its own row.

## Decision Outcome

Chosen: fold every run of 2+ steps (tool rows, mid-turn rings, thoughts, hidden rows) between
prose, with at least 1 tool row, also while the turn runs; hidden rows do not count toward the 2.
A fold is one line, so it changes text, not height, as steps land. One rule sets whether a fold
draws its steps: a fold never hides a group of rows the owner has already seen as rows. A fold
starts open if 2 or more of its shown steps were drawn before as rows, plain or as steps of an
open fold; otherwise it starts closed, unless a step failed. The owner's choice always wins. A fold with a
failed step says "N failed". The Zig parser tags a ring as `system_message` kind `swarm_ring`; a ring starts a turn
only when no turn is open. AGY writes no turn end, so the parser ends an AGY turn at a finished
reply that calls no tool. A step is failed when its log marks the result failed. Codex marks
every result completed, so only in a Codex log the result text decides: a non-zero exit code from
the result header or a printed JSON result, `Script failed`, or a rejected `Promise.allSettled`
entry makes a step failed. Each row has a Show Source action that shows the translated event JSON of
each source event of the row. Fold open state lives in memory for each open chat. Rejected: 0039's
fold, because a chat watched live never folds; a failed step that breaks the run, because a run
with failures turns back into many rows.

### Consequences

- Good: the agent's prose and last answer read without scrolling past tool rows; every source event
  stays one expand or one Show Source away.
- Good: a failed Codex exec is no longer drawn as finished.
- Bad: one click more to see a single step in a run of 2.
- Neutral: a fold's open state is gone when its chat is freed (ADR 0024, 0025).
- Neutral: an open fold's steps are rows of the upside-down list (ADR 0028), so a long open fold
  and Show Source build only what is on screen; the owner checks the window.
