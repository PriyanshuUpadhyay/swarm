---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0110"]
related: ["0122", "0083", "0107", "0167"]
informed-by:
  - 'owner, 2026-11-25: "for example right now continueing the same chat with a different model"'
  - 'owner, 2026-11-25: "is way to complex and changing model also starts clean or with a summary and it''s very user unfriendly let''s do"'
  - 'owner, 2026-11-26: "Yes, defaults okay." (a switch to another CLI keeps the same chat, tab and row, with no question per switch).'
---

# 0168. A switch to another CLI keeps the same chat and asks nothing

## Context and Problem Statement

ADR 0110 resumes the same CLI session on a model switch. When the CLI changes, its popover asks
each time whether to bring a summary or start clean, with the owner's saved default preselected,
a "Make default" button and the same entry in Settings. The owner finds this too complex and
unfriendly. The design file lists this rule as one that needs an ADR change before code.

## Decision Outcome

- A switch to another CLI keeps the tab, the sidebar row and the transcript. The new CLI session
  links to the same chat (`continuation_of`). The transcript route reads the chain from old to new.
- The same-CLI resume of 0110 stays.
- The app asks no question per switch. The whole chat goes to the new CLI as a file first (the
  handoff of 0122), a summary second, and no history third. The kept choice applies to the next
  switch.
- The foot of the picker (ADR 0167) names what the new CLI does not get: the thinking level, the
  tool state and the attached files.
- A usage limit offers "Switch model" beside "Switch account". It uses the chat file and does
  not wait for a summary.
- "Resume" of an ended chat goes on in the same chat with the picked model.

## Consequences

- Good, because the owner switches a CLI in one pick and does not lose the chat in the list.
- Bad, because one chat now holds sessions of more than one CLI. Code that reads one session per
  chat must follow the chain.
- Bad, because a CLI can lose context that cannot move (thinking, tool state, files).
- The switch is built in run 4. The line in the chat that shows the change is in ADR 0169.
