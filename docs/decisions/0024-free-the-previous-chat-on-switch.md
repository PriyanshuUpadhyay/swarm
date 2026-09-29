---
status: accepted
date: 2026-09-29
deciders: [user]
related: ["0022"]
informed-by:
  - "User answer on 2026-09-29: a workspace or chat switch frees the older chat's memory"
---

# 0024. Free the previous chat on switch

In the context of many live panes per chat, facing every opened pane keeping a `swarm attach`
process and its parser alive until the window closes, and a cache of three chat views, we chose
to load panes lazily, keep them while their chat is open, and stop them and drop the chat view on
a chat or workspace switch, and neglected an LRU of several chats, to keep memory flat as the
owner moves between chats, accepting that returning to a chat re-reads its transcript and
re-attaches its panes.
