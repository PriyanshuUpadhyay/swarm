---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0049", "0115", "0142"]
related: ["0054", "0055", "0116"]
informed-by:
  - 'owner, 2026-11-25: "Right now there''s a lot of clutter."'
  - 'owner, 2026-11-25: "We have the composer to start a particular project."'
  - 'owner, 2026-11-25: "At least if they are visible, they should be at the bottom."'
  - 'owner, 2026-11-26: "one list looks good"'
  - 'owner, 2026-11-24: "4th is good" (the row layout) and "not now" (tab groups).'
  - 'owner, 2026-11-24: "I think the way the item A shows needs use very good just that we can show a count of pains that need me that would be better."'
  - 'owner, 2026-11-25: "wait make side bar to be head b"'
  - 'owner, 2026-11-25: "rail" (a narrow window).'
---

# 0166. The sidebar is one list of active chats, and "Needs you" comes first

## Context and Problem Statement

ADR 0049 puts chat rows under their workspace, grouped by project, and keeps a project with all
its worktrees until "Remove project". ADR 0115 lists every chat of a workspace in the sidebar.
ADR 0142 lists main, the worktrees in use, and one row "N more worktrees". The owner finds the
sidebar cluttered and wants a count of the chats that need them.

## Decision Outcome

The sidebar is one list. There is no "Workspaces" and "Recent" switch.

- Order: "Needs you" (a count badge, an amber dot per row, a fold after five rows), "Pinned" when
  something is pinned, then the projects in a fixed order.
- A project shows only with an active chat: one that works, waits, failed, has an unread result,
  is pinned, or had activity in 7 days. The other projects fold into "N more projects". Search
  finds every chat.
- Each active chat is one row, newest first. A worktree with one chat is that chat's row. After
  five rows the list shows `… N more` and "Show all". A working, waiting or failed chat is never
  folded.
- A workspace with no chat, the row "N more worktrees" and the fold "Archived · N" leave the
  list. The "+" of a project opens the composer, which lists the worktrees ("+ > Existing
  branch" of 0142 stays). One "Archive" button opens the archive.
- Below 1024 px the sidebar is a 52 px rail of state marks. Tab groups stay out (0076 dropped
  them).

## Consequences

- Good, because the list is short and the chats that need the owner come first.
- Bad, because a project with no active chat is not in the list. The owner starts it from the
  composer or from search.
- Bad, because a new rail and a new "Needs you" group need new code.
- Open: this record does not decide chat pins. The code has no chat pin today. "Pinned" holds
  what the owner can pin today. A chat pin is a later decision.
- The sidebar is built in run 3.
