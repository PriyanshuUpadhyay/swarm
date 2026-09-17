---
status: superseded
superseded-by: 0013
date: 2026-09-17
deciders: [user]
related: [0003, 0007]
informed-by: []
---

# 0012. A workspace chat is the CLI agent in tmux

In the context of the app as a UI over tmux, facing a built-in chat where Esc did not stop the agent
and Stop put a queued message back in the text box, and agents that inherited the orchestrator's
Herdr identity, we chose to open new chats as the real CLI in a tmux pane, to have the text box
type into that pane and Esc send Escape to it, and to strip `HERDR_*`, `CLAUDE_CODE_*` and swarm
identity from every child the app starts, and neglected fixing Esc and Stop in the built-in chat,
so the chat behaves as the CLI does and a skill inside it splits panes in the same tmux window,
accepting that the built-in chat stays available only by the Settings choice and that chats made
before this change keep the built-in chat.
