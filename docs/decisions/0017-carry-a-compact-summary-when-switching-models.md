---
status: accepted
date: 2026-09-24
deciders: [user]
supersedes:
superseded-by:
related: [0016]
informed-by:
  - ui/Sources/SwarmCore/Agent/SwarmChatHandoff.swift
links: []
---

# 0017. Carry a compact summary when switching models

## Context and Problem Statement

Claude and Codex do not share one provider session. A new agent needs the prior chat's context
when the user changes models within a conversation.

## Considered Options

- Ask the old agent for a compact summary and send it to the new agent.
- Let the user write a handoff summary.
- Start the new agent without prior context.

## Decision Outcome

We chose an automatic compact summary. When the old chair is live, Swarm asks it for a handoff
summary. When its pane is gone, Swarm sends recent user and agent messages so the new chair can
compact them. Swarm links the sessions only after it sends that context.

### Consequences

- Good: the user can change providers without copying the chat history.
- Bad: a live switch adds a summary turn and can take longer; recent-message fallback can omit
  older details when the old chair is gone.
