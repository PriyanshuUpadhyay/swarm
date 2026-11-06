---
status: accepted
date: 2026-11-06
deciders: [user]
amends: "0044"
related: ["0044", "0077"]
informed-by:
  - 'owner, 2026-11-06: "We are not getting notified when the main orchestrator is done. We don''t want notification when the child is done, but we want notification when it asks a question. Not the multiple choice types, but when it is done in all the four agent types."'
---

# 0138. Swarm notifies when the app chair ends a turn

## Context and Problem Statement

ADR 0044 left the done notice to the chair model, which must run `swarm notify` at the end of its
run. The owner reports that this notice often does not come. A chair also ends a turn with a
question in plain text, and that question gets no notice, because only a multiple-choice prompt
reads as `waiting`. The app chair runs swarm's state hooks on all four CLIs (Claude, Codex, AGY by
hooks, pi by its status feed), so swarm sees each turn end.

## Decision Outcome

When the chair's stored state changes from `working` or `waiting` to `done`, swarm adds one notice
row, `<folder>: the orchestrator is done`. A child's turn end adds none. When a child is still
`working`, the chair's turn end adds none, because the chair only waits for that child's reply.
The notice is a done notice under ADR 0077, so the `done` notice preference can turn it off. The
app chair contract no longer tells the chair to run `swarm notify`, so one turn end posts one
notice. A hand chair has no state hook and keeps the 0044 rule.

### Consequences

- Good: the done notice and a plain-text question no longer depend on the model following a line.
- Bad: each chair turn end with no working child posts a notice, also a short reply in a talk.
- Bad: a child whose stored state stays `working` after its pane died holds back the notice until
  the screen check corrects that state.
