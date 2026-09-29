---
status: accepted
date: 2026-09-29
deciders: [user]
supersedes: "0024"
related: ["0022"]
informed-by:
  - "User answer on 2026-09-29: keep the rows cache with a smaller cap"
  - "Open-time measurement on 2026-09-29: warm reopen 0 ms with cached rows; cold chat open p95 797 ms"
---

# 0025. Keep the rows of four recent chats and free the rest

In the context of switching between chats, facing that a reopened chat with no kept state paid a
cold open of 300 to 900 ms while one with its last rows kept showed at once, we chose to keep only
the transcript rows of the four most recent chats and to free everything else of a previous chat
on switch (its panes, `swarm attach` processes, transcript readers, and poll task), and neglected
freeing all state (ADR 0024) and a 16-chat rows cache, to make a return to a recent chat instant
while memory stays bounded, accepting that a fifth chat and older ones open cold.
