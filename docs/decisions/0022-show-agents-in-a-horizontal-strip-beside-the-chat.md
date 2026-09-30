---
status: superseded
superseded-by: "0030"
date: 2026-09-29
deciders: [user]
related: ["0003", "0020"]
informed-by:
  - "User layout answer on 2026-09-29 (chat 90%, peek 10%, odd count 1 then 2 per column)"
---

# 0022. Show agents in a horizontal strip beside the chat

## Context and Problem Statement

The chat view shows live agent panes in a two-column grid, 320 pt tall, to the right of the
transcript. The owner wants the chat to own the width and still see that agents exist, and to
see many agents at once when needed.

## Considered Options

- Keep the two-column grid beside the transcript.
- Pane grid as the main view, chat secondary.
- One horizontal strip: the chat at 90% of the main area, then agent columns that scroll in.

## Decision Outcome

Chosen: the horizontal strip. The chat page is 90% of the main area; the first agent column
peeks into the last 10%. A column is 1/3 of the main area, at least 440 pt. With an odd count
the first column holds one full-height pane and later columns hold two; with an even count each
column holds two. Scrolling snaps to column edges, and ⌘↩ zooms one pane.

### Consequences

- Good: the chat reads at full width, and six panes fit on screen when you scroll right.
- Good: the layout is one pure function that tests can cover.
- Bad: you cannot see the chat and more than about one column of agents at the same time.
