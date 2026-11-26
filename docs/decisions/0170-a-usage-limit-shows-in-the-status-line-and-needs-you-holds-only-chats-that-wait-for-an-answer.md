---
status: accepted
date: 2026-11-26
deciders: [user]
related: ["0099", "0112", "0160", "0166"]
informed-by:
  - 'owner, 2026-11-25: "Yes, all eight defaults are ok (sidebar head A)", the answer after round 16. It covers the nine points of the board "B · open nine" of round 16 (`docs/design/boards/swarm-b-open-nine.fig`), and point 3 of that board is the usage limit.'
---

# 0170. A usage limit shows in the status line, and "Needs you" holds only chats that wait for an answer

## Context and Problem Statement

ADR 0099 limits the transcript to what the CLI shows. ADR 0112 shows limits in the run popover.
ADR 0160 lets a ring wait for an open question and counts a failed child as a stall. ADR 0166
adds the group "Needs you" to the sidebar. The design needs a place for a usage limit that is
not the chat, and a rule for what "Needs you" holds. This record reverses no sentence of those
records: the status line is outside the transcript that 0099 limits.

## Decision Outcome

- A usage limit shows in the status line, not in the chat: an amber dot, the text "Waiting for the
  limit · resets 16:00" (the time is an example) and the quiet link "Switch account".
- "Needs you" holds only chats that wait for an answer: a question or a permission. A state
  change and a limit are not in it. A chat with a limit keeps its amber dot in its project group.

## Consequences

- Good, because the group "Needs you" means one thing: the agent waits for the owner.
- Bad, because a limit is easy to miss when the status line is not in view.
- Open: the source of the limit signal is not decided here. The data contract of run 5 decides
  whether the web derives it from the account meter or a new state kind is added.
- The status line is built in run 5. The group itself is built in run 3 under ADR 0166.
