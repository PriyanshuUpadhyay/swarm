---
status: accepted
date: 2026-10-28
deciders: [user]
amends: ["0038"]
related: ["0021", "0012"]
informed-by:
  - 'owner, 2026-10-28, in chat: "It is a normal message. Whatever state comes up, it should just render that. We are not supposed to do any conditional rendering anywhere, because that is all handled by the CLI."'
  - "incident 2026-10-28: a pi chair kept a Sent /compact bubble after the compaction, because pi logs no user row for /compact and the next turn began 18 ms after the compaction ended"
  - "council 2026-10-28-sent-rows-cli-state (Claude, GPT, Gemini): GO, unanimous at round 2; ~/dotfiles/docs/council/2026-10-28-sent-rows-cli-state.md"
  - "pi 1.2.0 interactive-mode.js updatePendingMessagesDisplay: Steering and Follow-up lines and a dequeue hint above the editor; the list includes messages held during compaction"
---

# 0130. The chat shows a sent message only as the CLI shows it

## Context and Problem Statement

ADR 0038 kept an app-made "Sent" row for each message that Codex, AGY, and pi had not logged yet.
The app removed the row when a log row showed the same text, or when a turn that ran after the
send ended. Each CLI needed its own match: a pi skill message logs as a command row, and pi logs
`/compact` only as a compaction entry. A pi chat kept a "Sent /compact" bubble because pi started
the next turn 18 ms after the compaction, so the pane never showed idle.

## Considered Options

- Keep the Sent row and add a match for each CLI path, such as a compaction row for `/compact`.
- Show only what the CLI shows: its log, and for pi, the queue that pi draws on its screen.

## Decision Outcome

Chosen: show only what the CLI shows. The app makes no Sent row for any CLI. Claude's queue
rows still come from its log (ADR 0038). `swarm agents --json` gives a pi agent a `queue` list
read from pi's screen, one entry per `Steering:` or `Follow-up:` line, and the chat shows each as
a queued row with pi's own word as its label. Codex and AGY show a message when their log
records it. The app keeps the exact texts it typed into pi only as input memory, so Stop and Up
put back the exact text, line breaks and all; that memory never makes a row. Rejected: a match
for each CLI path, because each new path needs one more rule.

### Consequences

- Good: the chat and the pane show the same queue, and no app-made bubble can outlive it.
- Good: a message typed while pi compacts shows as pi's own queued line.
- Bad: Codex and AGY show nothing between the send and the log row, so the owner may send twice.
- Bad: pi's queue lines are cut to one row, so a long queued message shows only its start.
- Bad: the queue read follows pi's screen text, so a pi update can change it.
