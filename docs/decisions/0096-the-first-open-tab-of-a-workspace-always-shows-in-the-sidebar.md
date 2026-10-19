---
status: accepted
date: 2026-10-19
deciders: [user]
related: ["0085", "0093"]
informed-by:
  - 'owner, 2026-10-19 (paraphrase): there is a chat in main but it does not show here; the sidebar item can be the first tab that is open in that workspace'
  - 'owner, 2026-10-19 (paraphrase): if there are more tabs, then after an archive whichever is the first chat should show there'
  - src/view/tree.rs (Workspace::listed_chats)
---

# 0096. The first open tab of a workspace always shows in the sidebar

## Context and Problem Statement

ADR 0085 keeps a chat started in a tab out of the sidebar while its tab is open. A workspace
whose only chats were started with ⌘T or the strip's "+" then showed "0 chats" and no row under
it, while its tab strip held a live chat. The sidebar gave no way to reach that chat.

## Considered Options

- The first open tab of the workspace strip always shows in the sidebar, whether or not it is a
  tab chat.
- A tab chat shows only when the workspace lists no other chat.
- Tab chats stay hidden in all cases (ADR 0085 as written).

## Decision Outcome

Chosen: the first option, because every workspace with an open tab then has a row that opens it,
and the rule needs no other state. The rest of ADR 0085 and ADR 0093 stays.

- `Workspace::listed_chats` lists a chat whose key is the first key of the workspace strip's open
  tabs (the saved strip, else the seeded one), even when the chat is in `tab_chats`.
- The rule reads the strip as it is now. When the first tab closes or its chat is archived, the
  next tab becomes the first, so its chat shows.

### Consequences

- Good: a workspace with only tab chats shows one row and a count of at least 1.
- Good: the row count, status and badges follow from `listed_chats`, so they stay in agreement.
- Bad: which tab chat shows depends on the tab order, so a tab move can change the sidebar row.
- Bad: closing the shown tab chat's tab still archives it (ADR 0085), so its row goes away.
