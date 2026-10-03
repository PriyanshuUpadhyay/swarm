---
status: accepted
date: 2026-10-03
deciders: [user]
related: ["0012", "0023"]
informed-by:
  - "User direction on 2026-10-02 (flow composer-gaps): follow each CLI's steer and queue, show a queued message, edit it with Up like the CLIs; let the CLI handle /clear, with no new GUI chat"
  - "Claude Code 2.1.287 logs: queue-operation records enqueue, dequeue, remove (absorbed_mid_turn), popAll; Up with an empty box pops the whole queue into the input box"
  - "Codex CLI: Enter steers into the running turn, Tab queues; the rollout log has no queue records"
  - "docs/research/2026-10-02-cli-composer-edge-cases.md"
---

# 0038. The composer follows each CLI's queue

## Context and Problem Statement

The composer types into the agent CLI in its tmux pane (ADR 0012). A message sent while the agent
works is queued or steered by the CLI, not by the app, so the app did not show it, and the owner
could not take it back to edit it. `/clear` made the app open a new GUI chat, apart from what the
CLI did.

## Considered Options

- An app-owned queue that holds the text until the turn ends, then types it.
- Follow each CLI: show what the CLI queued, and pull it back with the CLI's own keys.

## Decision Outcome

Chosen: follow each CLI. For Claude, the app reads the queue records in the log, shows each owner
message as a "Queued" row with an "↑ to edit" hint, and Up pulls the messages back by pressing Up
and then C-u in the pane, never Escape or C-c. For Codex and AGY, which write no queue records, the
app keeps a "Sent · joins after the next tool call" row until the message shows in the log or the
turn ends. `/clear` goes to the CLI; the chat stays in its tab, and a "Context cleared" divider
follows the earlier rows. Rejected: the app-owned queue, because it changes when a steer reaches
the agent.

### Consequences

- Good: a message reaches the agent at the moment the CLI would deliver it, as in the terminal.
- Good: Up edits a queued message, as in the CLI.
- Bad: a pull-back can race the CLI; the app reads the log after Up and reports "Already sent" or
  "Could not confirm" instead of guessing.
- Bad: the app does not press Up while the queue holds the agent's own entry (a task
  notification), because Up would take it out of the queue.
- Bad: the rules depend on each CLI's log format, so a CLI update can change them.
