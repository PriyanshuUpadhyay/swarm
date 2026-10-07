---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0023", "0025", "0049", "0052", "0055"]
informed-by:
  - "Owner verdicts on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-7 (b), T-1, T-2, T-3, T-4, T-5, T-9, T-14, T-18 don't, TC-7"
  - "SessionsTree.swift:256 sorts tabs by last bus message, so ⌘1–9 targets drift"
---

# 0054. Tabs are the chats the owner opened in a workspace, in the owner's order

## Context and Problem Statement

Every chat in the selected workspace was a tab, sorted by live state and last bus message. A
message from a child moved its tab, so ⌘2 opened another chat. A tab could not be dragged or
grouped, and the only ways to remove one were Close, which ended the chat, and Archive. Now that
the sidebar lists every chat (ADR 0049), the strip can hold fewer.

## Considered Options

- Every chat in the workspace, sorted by activity, as before.
- An open set for each workspace, with a manual order.
- One global tab set across workspaces.

## Decision Outcome

Chosen: an open set for each workspace. Opening a chat from the sidebar adds its tab to the right
of the current tab. Closing a tab only hides it; the chat keeps running and stays in the sidebar.
The owner drags tabs to reorder them and into named colour groups that fold to one chip. A tab
with children shows a count badge such as "3 · 1 waiting", and a click jumps to the waiting
child. An ended chat's tab can be closed and hides when the owner leaves it. Tabs past the edge
show in an overflow menu. After a tab closes, the chat seen before it is selected. Rejected: one
global set, because switching workspace should switch context.

### Consequences

- Good: tabs stay where the owner put them, and ⌘1–9 stay stable.
- Good: a running chat can leave the strip without being stopped.
- Bad: the open set, order and groups are new state for each workspace that must be stored and
  pruned.
