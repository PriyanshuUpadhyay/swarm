---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0007", "0021", "0038"]
informed-by:
  - "Owner answers on 2026-10-05 (flow tmp/flow/2026-10-05-ring-proof, 03-contracts): Q1 (a) the proof wait runs inline in every ring, the app listing included; Q2 (a) a lost ring to the chair, or a chair idle with unacked work, goes to stderr and a `swarm sweep` line only"
  - "src/main.rs ring_pane at main 8bdeec42: only a ring to Claude waits for its input box and presses Enter again"
  - "ADR 0038: a ring to a busy CLI is queued or steered, so it fires no turn-start hook"
---

# 0041. A ring is proven by a turn start, and a stalled agent is reported to the chair

## Context and Problem Statement

A ring to Codex or AGY was typed and forgotten. Only Claude got a second Enter. Nothing recorded
whether a ring started a turn, and after 2 rings a lost message was silent. A child at `done` with
work in its inbox was never flagged.

## Considered Options

- Every ring waits for proof inline, up to its 30 s deadline, the app's listing included.
- CLI rings wait inline; the app's listing types the ring and its next pass resolves proof.
- No ring waits; every sweep and listing pass resolves proof from the store and the screen.

## Decision Outcome

Chosen: every ring waits inline, because one code path serves `send`, `finish`, `ack`, `sweep`, and
the listing, and `swarm send` can report the result. A ring returns `hook` (a turn-start hook after
the ring second), `screen` (a working or waiting screen), `unconfirmed`, or `unchecked` (no proof
can exist), and stores it on `message.delivery`. While the composer still holds the ring text, swarm
presses Enter again, for each provider whose composer it can read. After 2 unconfirmed rings the
sender's chair gets `unconfirmed:<seq>`. `swarm sweep` and the listing send `stall:unacked:<seq>`
and `stall:silent:<seq>` once each, deduped by a unique index. The chair is never the subject of a
message: a lost ring to the chair goes to stderr and a `swarm sweep` line only, and a later notify
flow turns it into a user notice. Rejected: lazy proof, because it needs a second proof path and
`swarm send` could not report.

### Consequences

- Good: every provider gets a checked ring, and the chair hears of a lost message or an idle child.
- Bad: `swarm send` returns after proof, not after typing; about 1-3 s normally, up to 30 s for a
  lost ring.
- Bad: the app's listing can wait up to 30 s per lost re-ring, at most twice per message. Move the
  listing to lazy proof if the strip is seen to freeze.
- Bad: Codex screen proof rests on synthesized patterns until real captures exist.
