---
status: accepted
date: 2026-09-16
deciders: [user]
related: [0001]
informed-by: []
---

# 0003. Show each swarm agent as a chat and a live pane

In the context of showing swarm agents in the Bloom UI, facing that Bloom's chat uses JSON agent
modes while swarm runs interactive CLIs in tmux panes, we chose two views per agent, a chat built
from swarm messages and a SwiftTerm pane attached to the agent's live tmux pane, and neglected a
terminal pane only and a chat only, to give a friendly view for non-technical users and full terminal
control for Claude, Codex, and AGY alike, accepting that we build and keep two views in step.
