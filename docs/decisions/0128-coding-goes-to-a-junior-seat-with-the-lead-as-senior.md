---
status: accepted
date: 2026-10-27
deciders: [user]
related: ["0124"]
informed-by:
  - "Council 2026-10-26 roles-next-steps (GPT, Gemini, Claude seats); log in the owner's ~/dotfiles/docs/council/2026-10-26-roles-next-steps.md"
  - "Role probes a-d, 2026-10-26; ~/.claude/reports/2026-10-26-roles-probes/notes.md"
  - "A/B/C test on 8 replayed tasks, 2026-10-26 to 2026-10-27; ~/.claude/reports/2026-10-26-abc-test/notes.md"
  - "owner, 2026-10-27, in chat: keep B and write the ADR"
---

# 0128. Coding goes to a junior seat, with the lead as the senior who answers and takes over

## Context and Problem Statement

A chair sends coding to one seat. A cheap `code.routine` seat (Sonnet) costs less per token, but it
can miss a requirement and has no one to ask. A `code.complex` seat (Codex `gpt-6.1-sol`) is
stronger, but its sandbox has no network, and it rewrote tests outside its task. The kit blocks
now give the lead three moves: it answers a junior's ask from the board, it sends a result back
for revision, and it takes the task to `code.complex` after two red revisions (PR #157).

We tested three routes on 8 replayed tasks with fixed oracles (hidden pass checks). A was the junior
alone, B was the junior with the lead's moves, and C was `code.complex` alone. One task (t5) had no
fair oracle. Of the other 7, A passed 4, B passed 6, and C passed 5. B passed two tasks that A
failed (t3, t15). B failed one task that C passed (t2, where the lead's answer sent the coder to a
different cause).

## Considered Options

- Junior plus lead (B): `code.routine` seat; the lead answers asks, revises, and takes over (chosen).
- Junior alone (A): `code.routine` seat with no lead moves.
- Senior alone (C): every coding task on `code.complex`.

## Decision Outcome

Chosen: junior plus lead, because it passed the most tasks (6 of 7) and had the lowest Claude cost
per accepted task ($5.28, against $7.45 for A). The council's keep rule asked for zero C tasks
lost; B lost one. The owner accepted that loss over a third batch.

### Consequences

- Good: a junior that misses a requirement gets one more chance through the lead's revision, and a
  stuck junior hands off to `code.complex` with its result as the brief.
- Good: `swarm close` releases the closed seat's claims, so a takeover claims the work at once.
- Bad: the lead is 40-60% of each run's cost, so the saving depends on the lead staying short.
- Bad: the sample is 7 tasks. The cost of B against C is unverified, because Codex tokens have no
  price here. The t2 loss shows that a lead's answer can mislead a junior.
- Bad: no code triggers the takeover yet. The lead applies the rule by hand until three useful hand
  interventions are seen, as the council set.
