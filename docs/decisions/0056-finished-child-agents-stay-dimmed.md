---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0029", "0030"]
informed-by:
  - "Owner verdict on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-9 (b), T-10"
  - "SwarmSessionDetail.swift:332 keeps only live agents, so the ended dimming at PaneStrip.swift:218 never runs"
---

# 0056. Finished child agents stay dimmed until dismissed

In the context of the pane strip, facing a child's column that left the moment its pane exited,
so its final answer could not be read, we chose to keep a finished child's column dimmed with its
log until the owner dismisses it with its × or "Clear finished", and to dim its row under the chat
in the sidebar, and neglected removing it at once and folding finished children into one chip, to
achieve a readable final answer with mostly a deletion of the live filter, accepting that a chat
with many finished children fills the strip until it is cleared.
