---
status: accepted
date: 2026-10-19
deciders: [user]
related: ["0085", "0083"]
informed-by:
  - 'owner, 2026-10-19 (paraphrase): the swarm/main row spins as working, but the one chat under it is done, and the working chat is not visible'
  - 'live state, 2026-10-19: chat 01a15324 was live with 18 agents and in tabChats, but not in the workspace tab strip and not archived'
  - src/view/tree.rs (Workspace::listed_chats, Workspace::status)
---

# 0093. A workspace row reads only the chats it lists, and a tab chat without its tab is listed

## Context and Problem Statement

ADR 0085 keeps a chat started in a tab out of the sidebar, and makes a workspace row count only
the chats the sidebar lists. The chat count obeyed that rule, but the row's status, badges and age
still read every chat. So the `main` row showed a spinner and "1m" for a hidden chat, while the
one chat under it was done and six minutes old.

ADR 0085 also relied on the rule that closing a tab chat's tab archives it. A live tab chat lost
its tab without an archive, so it showed in no tab and in no sidebar row. The row's spinner was
its only trace. We did not find the code path that dropped the tab.

## Considered Options

- The row reads only listed chats, and a tab chat is listed again while its tab is not open in
  the workspace strip.
- The row reads only listed chats, and tab chats stay hidden in all cases (ADR 0085 as written).
- The row keeps reading every chat, so a hidden chat still moves the spinner.

## Decision Outcome

Chosen: the first option, because the row then always agrees with the list under it, and a live
chat can never be invisible. This reverses the option that ADR 0085 neglected ("hiding a tab
chat's row only while its tab is open") for this case. The rest of ADR 0085 stays.

- `Workspace::listed_chats` hides a chat only when its key is in `tab_chats` and in the
  workspace strip's open tabs (the saved strip, else the seeded one).
- `Workspace::status`, the row badges, the row age and the workspace fields all read
  `listed_chats`.

### Consequences

- Good: the spinner, the badges and the age on a workspace row match the chats under it.
- Good: a tab chat whose tab leaves the strip by any path comes back in the sidebar.
- Bad: a tab chat can show as a sidebar row, which ADR 0085 meant to prevent, until it is
  archived or its tab opens again.
- Bad: the path that drops a live tab chat's tab without an archive is still unknown.
