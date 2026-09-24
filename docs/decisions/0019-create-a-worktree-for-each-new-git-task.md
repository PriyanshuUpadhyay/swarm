---
status: accepted
date: 2026-09-24
deciders: [user]
related: [0011, 0013, 0014]
informed-by: ["User choices for New Task, adjacent worktrees, and chat launch", "https://www.conductor.build/docs/concepts/git-worktrees"]
---

# 0019. Create a worktree for each new Git task

## Context and Problem Statement

Swarm can open Git projects and start chats in an existing folder. The owner wants a Conductor-like
task flow where separate tasks can change the same repository without sharing one checkout.

## Considered Options

- Create a branch and worktree for every new chat.
- Add a separate New Task action while New Chat keeps its current folder.

## Decision Outcome

We chose New Task for Git projects. It creates a branch from the project's default branch in an
adjacent worktree, then opens New Chat there. A bare repository hub uses its `wt` directory; an
ordinary checkout uses a sibling `<project>-worktrees` directory. Plain folders keep New Chat
without Git setup.

### Consequences

- Good: independent tasks get separate files and branches while existing chats can stay in one
  worktree.
- Bad: each task uses disk space and needs later review and worktree cleanup; Swarm does not yet
  automate those later steps.
