---
status: accepted
date: 2026-09-17
deciders: [user]
related: [0003, 0005]
informed-by: []
---

# 0007. Bloom is the chair for the swarm agents it starts

In the context of chatting with swarm agents in Bloom (ADR 0003), facing the need for one agent to
own each session and receive every reply, we chose to make Bloom the orchestrator of the agents it
starts, so the user types an ask in an agent's chat and its `swarm finish` reply shows there, with
each agent in its own tmux session on a private socket so its pane can be attached alone, and
neglected an orchestrator agent that the user chats with and that briefs the workers, and a
watch-only view of sessions started in Herdr or tmux, to work like Conductor with the smallest
change, accepting that Bloom must run the sweep itself and that Herdr-hosted sessions are not shown.
