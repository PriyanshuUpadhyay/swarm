---
status: accepted
date: 2026-10-14
deciders: [user]
supersedes: "0056"
related: ["0029", "0030"]
informed-by:
  - 'owner, 2026-10-14: "Don''t show the list of the agents of the chat and don''t keep a historical log of the empty panes. We only need active sub-agents to appear."'
  - 'owner, 2026-10-14: "If a chat cannot be loaded, just show its log so that the user has context. They should also be able to restart it."'
---

# 0083. Only live child agents show, and a closed chat shows only its log

In the context of the pane strip, the sidebar and the chat page, facing a strip and a child list
full of ended agents and a disabled composer on chats whose chair pane had closed, we chose to show
a child agent only while its pane is alive (in the strip, the sidebar rows and the counts), and to
show a chat whose chair has no live pane as its log with one "Continue chat" button that opens
Switch model, and neglected keeping finished children dimmed until dismissed (ADR 0056), to achieve
a view of only the work that can still act, accepting that a finished child's final answer is read
in the chair's log or with `swarm attach`, not in its own column.
