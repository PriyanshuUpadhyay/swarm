---
status: accepted
date: 2026-11-19
deciders: [user]
amends: ["0145"]
related: ["0144", "0150"]
informed-by:
  - 'owner, 2026-11-19: "land should not automatically do the cleanup.. after land the agent should offer the cleanup and when approved then it should do the worktree removal"'
---

# 0151. `swarm task land` keeps the worktree until the owner approves cleanup

## Context and Problem Statement

ADR 0145 made `swarm task land --merge` remove the task worktree and its record after the push. The agent had no moment to tell the owner what landed, and the owner had no say in the removal.

## Decision Outcome

No form of `swarm task land` removes the worktree or the task record. This includes `--merge` with and without origin.
The success answer has a `next` field. It tells the agent to ask the owner, and after approval to run `swarm task drop <branch>` (after a direct merge) or `swarm task sweep --apply` (after a PR merges).
A `land --merge` run again after its wait ran out finds the landed branch and returns the waiting answer.
The landing request file format does not change.

## Consequences

- Good: the owner approves each removal, and the code tells the agent so.
- Bad: a landed worktree stays on disk until someone runs `drop` or `sweep`.
- Neutral: the task record stays, because `drop` needs it.
