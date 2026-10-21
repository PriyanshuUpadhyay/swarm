---
status: accepted
date: 2026-10-21
deciders: [user]
amends: ["0060"]
related: ["0054"]
informed-by:
  - 'owner, 2026-10-21: "need cmd shift w to archive the opened chat"'
  - "web/src/core/app-actions.ts held ⇧⌘W as a Close Window placeholder for S6; no code closed the window"
---

# 0103. ⇧⌘W archives the open chat

In the context of the key map, facing an owner who wants one key to archive the open chat, we
chose ⇧⌘W for Archive chat, with the same confirm for live agents as the palette's Archive chat,
and neglected the ⇧⌘W Close Window that ADR 0060 planned but never built, to put archive next to
⌘W (hide the tab), accepting that Close Window has no key until one is chosen.
