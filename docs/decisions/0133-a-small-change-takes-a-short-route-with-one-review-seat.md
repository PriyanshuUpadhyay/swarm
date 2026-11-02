---
status: accepted
date: 2026-11-02
deciders: [user]
related: ["0132"]
informed-by:
  - 'trial 5 (~/.swarm/ws/trial5/RESULT.md): a 35-line CLI flag took 15.5 min and 6 seats with the kit, 1.5 min with no kit, same score'
  - 'council 2026-11-02 (~/dotfiles/docs/council/2026-11-02-kit-right-size.md), GO-WITH-CHANGES unanimous: a small route first, with one independent review and real tests'
  - 'owner, 2026-11-02: approved plan tmp/plan/2026-11-02-kit-right-size in round 1, "Do the changes then run the trials"'
---

# 0133. A small change takes a short route with one review seat

## Context and Problem Statement

Under ADR 0132, every verb runs the plan front door: a plan page, an owner round, seven step files,
and a full seat map. For a small change, that overhead was about 10x the work, with no gain in score.

## Considered Options

- A small route file for `implement`, with an intake gate, a post-build size script, and one fresh
  review seat (chosen).
- The fast lane alone. It skips three steps but keeps the plan page, the step files, and the seats.
- No gate. Every change takes the full route.

## Decision Outcome

Chosen option: the small route in `skills/kit/presets/implement/references/small.md`, because it
cuts the front door and the seats but keeps a real test and an independent review.

- `size_gate.py <base>` reads the real diff after the build. It raises at 4 or more files, more
  than 100 lines, or a schema or migration path. The limits are fixed in the script.
- A raise sends the change to the full `implement` preset.

### Consequences

- Good, because a small change costs about one agent and one review seat.
- Bad, because the intake gate is a judgment. The size script catches only size and schema, not a
  hidden contract change in a small diff.

## Confirmation

`skills/kit/presets/implement/scripts/test_size_gate.py` prints `ok`. Trial 6 measures the time.
