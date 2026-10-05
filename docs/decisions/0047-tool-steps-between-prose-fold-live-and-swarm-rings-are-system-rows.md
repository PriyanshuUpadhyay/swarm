---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0028", "0039"]
informed-by:
  - "Owner request 2026-10-05: chat readability while keeping everything reviewable"
  - "Owner answers 2026-10-05 (accept all recommendations): Q1 A live fold of 2+ steps, a failed step opens its fold; Q2 A parser kind swarm_ring; Q3 A Show Source shows the translated event JSON; Q4 A fold state in memory per open chat; a failed Codex exec shows as failed"
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
prose, also while the turn runs. A fold is one line, so it changes text, not height, as steps land,
and no visible group of rows ever collapses. A fold with a failed step opens by default and says
"N failed". The Zig parser tags a ring as `system_message` kind `swarm_ring`; a ring starts a turn
only when no turn is open. A non-zero exit code, including Codex `exit_code` and `Script failed`,
makes a step failed. Each row has a Show Source action that shows the translated event JSON of
each source event of the row. Fold open state lives in memory for each open chat. Rejected: 0039's
fold, because a chat watched live never folds; a failed step that breaks the run, because a run
with failures turns back into many rows.

### Consequences

- Good: the agent's prose and last answer read without scrolling past tool rows; every source event
  stays one expand or one Show Source away.
- Good: a failed Codex exec is no longer drawn as finished.
- Bad: one click more to see a single step in a run of 2.
- Neutral: a fold's open state is gone when its chat is freed (ADR 0024, 0025).
