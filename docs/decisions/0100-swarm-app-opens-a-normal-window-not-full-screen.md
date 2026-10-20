---
status: accepted
date: 2026-10-20
deciders: [user]
amends: ["0095"]
related: []
informed-by:
  - 'owner, 2026-10-20: "it should not open it in full screen when doing swarm ui"'
---

# 0100. Swarm.app opens a normal window, not full screen

In the context of `swarm ui` opening Swarm.app, facing an owner who no longer wants the window to
take a whole space, we chose a normal centered window that the owner can put in full screen with
⌃⌘F or the green button, and neglected the full screen at the first activation that ADR 0095 set,
to keep the owner's other windows in view, accepting that the camera-row strip of ADR 0095 shows
only after the owner enters full screen.
