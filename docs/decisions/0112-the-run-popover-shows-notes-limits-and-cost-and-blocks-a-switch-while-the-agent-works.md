---
status: accepted
date: 2026-10-22
deciders: [user]
amends: ["0110"]
related: ["0109"]
informed-by:
  - "~/.claude/reports/2026-10-21-model-composer/designs/index.html (design A of the composer mock)"
  - 'owner, 2026-10-22: "yes both /skill:implement auto"'
  - 'owner, 2026-10-22: "just disallow switch when the agent is working", after three review rounds each found a gap in a queued switch'
  - 'owner, 2026-10-22: "keep" (an agent that waits on a question may still switch)'
---

# 0112. The run popover shows notes, limits, and cost, and blocks a switch while the agent works

## Context and Problem Statement

ADR 0110 gave each chat one run chip and one popover that switches the model, the thinking level,
and the account. Design A of the composer mock also shows each model's note, each account's
limits, what a change costs, and a change made while the agent works. The owner must see all of
this before a click.

## Considered Options

- Design A with a queued change: a change made while the agent works waits as "Pending" and
  starts when the turn ends.
- Design A with a block: Apply is off while the agent works.

## Decision Outcome

Chosen: design A with a block, because each review round of the queued change found a new race
between the turn end, the pane restart, and the chat list (a change that started in a permission
dialog, after End or Archive, or that was lost after a completed switch). The base rule stays:
`applyBlock` says "Wait for this turn to end, or stop it." while the agent is working. An agent
that waits on a question may still switch, as before.

The popover shows:
- each model's note (Codex `description`, a fixed Claude alias list) and its default level from
  `/api/providers`, absent when the CLI gives none;
- each account's 5h and week limits from the cached `/api/usage` meters, with a color dot derived
  from the account name, also on the chip;
- a "What happens" line that names the action and its cost (a restart re-reads the whole chat
  once), and an Apply label that names the action ("Restart and apply", "Start new chat");
- "new chat" or "new chat + summary" on the group of another CLI.

The search box and the Bring over choice of ADR 0110 stay.

### Consequences

- Good: no switch state lives only in the browser, so a reload or a second window cannot start
  an old change.
- Bad: the owner waits for the turn to end, or stops it, before a switch.
- Bad: AGY and pi models show no note, because their CLIs give none.

## Links

- Amends ADR-0110 (the run chip and its popover).
