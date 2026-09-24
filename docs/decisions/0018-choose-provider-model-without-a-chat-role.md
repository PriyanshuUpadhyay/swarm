---
status: accepted
date: 2026-09-24
deciders: [user]
supersedes: 0004
related: [0005, 0008, 0017]
informed-by: ["User screenshot of the New Chat role picker", "Claude Code model configuration", "Installed Codex and AGY CLI model lists"]
---

# 0018. Choose a provider and model without a chat role

## Context and Problem Statement

New Chat exposed routing roles as model choices. Several roles repeated one model, and the menu
could not show all models accepted by the provider CLIs. The user wants to choose a provider and
model directly. The account Auto choice from ADR 0004 remains useful.

## Considered Options

- Keep the role picker and add a separate model override.
- Remove roles from chat setup and launch a direct chat model.

## Decision Outcome

We chose a provider and model picker with an editable model name. Codex and AGY supply model lists;
Claude supplies documented aliases because its CLI has no model-list command. The chat launch uses
the chosen model without changing routed role settings. Auto still chooses a signed-in account.

### Consequences

- Good: a chat can use a model that has no routed role, and the menu has no duplicate role entries.
- Bad: a typed model name may fail when the selected account lacks access, and CLI model lists can
  change outside Swarm.
