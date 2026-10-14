---
status: accepted
date: 2026-10-14
deciders: [user]
related: ["0083"]
informed-by:
  - 'owner, 2026-10-14 (paraphrase): list only the chats that swarm started through its own tmux adapter, not chats from tmux, Herdr or any other adapter; rename "tmux solo" to "tmux-swarm".'
---

# 0084. The app lists only the chats it started, on its tmux-swarm adapter

In the context of the sidebar, facing a list of 40 chats where most were lanes that a CLI in a
Herdr or tmux pane opened and that the app cannot drive, we chose to list a chat only when its
session runs on the app's own adapter, `tmux-swarm` (formerly `tmux-solo`, renamed in the database
by migration 0011), and neglected listing every lane in the workspace, to achieve a sidebar of the
chats the app started and can type into, accepting that a lane opened in a Herdr or plain tmux pane
does not show in the app until an import exists, while a workspace delete still closes its live
agents (ADR 0083).
