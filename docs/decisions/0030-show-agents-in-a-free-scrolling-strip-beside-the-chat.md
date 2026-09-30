---
status: accepted
date: 2026-09-30
deciders: [user]
supersedes: "0022"
related: ["0003", "0020", "0026"]
informed-by:
  - "User answer on 2026-09-30: snap is not a good ux. let it be free scroll"
  - "Screen recording 2026-09-30 2.55.50 PM: short swipes on the chat page did nothing"
---

# 0030. Show agents in a free-scrolling strip beside the chat

## Context and Problem Statement

ADR 0022 put the agents in a horizontal strip beside the chat and made the strip snap to column
edges. The chat page is 90% of the main area, so on a 1,100 pt area a swipe had to project past
495 pt before the snap left the chat page. A short swipe snapped back and looked like it did
nothing.

## Considered Options

- Keep the snap to the nearest column edge (`.viewAligned`).
- Keep a snap, but let the swipe direction pick the next column edge.
- Free scroll: the strip stops where the swipe leaves it.

## Decision Outcome

Chosen: free scroll, because the owner wants the strip to follow the trackpad like any other
scroll view, and a snap of either kind felt wrong.

The layout of ADR 0022 stays. The chat page is 90% of the main area; the first agent column
peeks into the last 10%. A column is 1/3 of the main area, at least 440 pt, or the width the
owner drags (ADR 0026). With an odd count the first column holds one full-height pane and later
columns hold two; with an even count each column holds two. ⌘↩ zooms one pane. Keys that move
focus still scroll the focused column into view.

### Consequences

- Good: a short swipe moves the strip by a short distance, and the owner can stop anywhere.
- Good: less code, because no scroll target rule exists.
- Bad: the strip can rest mid-column, so a pane can show cut off at the window edge.
