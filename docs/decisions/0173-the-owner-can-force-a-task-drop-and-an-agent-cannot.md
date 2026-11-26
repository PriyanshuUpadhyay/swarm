---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0135"]
related: ["0144", "0151"]
informed-by:
  - 'owner, 2026-11-26: "task drop does not work when squash merging or rebase merging... add a --force flag for the user to execute it... agent cannot execute but just suggest"'
---

# 0173. The owner can force a task drop, and an agent cannot

## Context and Problem Statement

`swarm task drop` refuses a task whose branch has commits that the default branch lacks (ADR 0135).
A squash merge or a rebase merge puts the work in the default branch under new commit ids, so
`drop` refuses a task that did land.

## Decision Outcome

`swarm task drop [<name>] --force` skips the commit check, removes the worktree and its record, and
deletes the branch.
It still refuses a worktree with uncommitted changes.
In an agent's shell the command refuses `--force` and tells the agent to ask the owner to run it.
An agent's shell is one with a variable of `AGENT_MARKERS`, or `PI_CODING_AGENT`.
The refusal of a plain `drop` names the forced command, so the agent can suggest it.

## Consequences

- Good: the owner removes a squash-merged or rebase-merged task with one command.
- Bad: `--force` does not check that the work landed, so the owner can delete unmerged commits. Git keeps them in its reflog for a time.
- Neutral: `swarm task sweep --apply` still removes a task whose PR merged, with no force.

## Confirmation

`a_forced_drop_removes_a_task_with_commits_main_lacks` in `src/task/tests.rs` and
`tests/task_drop.rs` own the two behaviours.
