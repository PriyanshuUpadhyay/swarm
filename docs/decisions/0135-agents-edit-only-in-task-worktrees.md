---
status: accepted
date: 2026-11-03
deciders: [user]
related: ["0019", "0040", "0042", "0046", "0055"]
informed-by:
  - 'owner, 2026-11-03: "I would always start the chat in main, always, or the default branch." "I will block any edits made to main by agents." "It will create the workfree itself, do it, do the cleanup, and then go away" "if tools exists for good grouping and working with worktrees we use them"'
  - 'research 2026-11-03 (~/.claude/reports/2026-11-03-agent-worktree-workspaces.md), design A'
  - 'council 2026-11-03 (~/dotfiles/docs/council/2026-11-03-agent-worktree-tools.md): adopt Worktrunk; land outside the agent sandbox; no wt step prune'
  - 'sandbox trial 2026-11-03 (tmp/flow/2026-11-03-swarm-swarm-workspace-mode/03-contracts.md): Codex profile passes; Claude sandbox denies package caches; srt blocks a nested Seatbelt'
---

# 0135. Agents edit only in task worktrees, and main is read-only for them

## Context and Problem Statement

The owner starts every chat in a hub's main worktree (`wt/main`, the default branch). An agent
there edited and committed on main in place. The owner wants each agent to make its own worktree,
work there, land it, and clean up. Worktrunk (`wt`) already creates, merges, and removes
worktrees.

## Considered Options

- A task worktree per edit, made and landed by `swarm task`, with Worktrunk underneath, and main
  read-only for agents (chosen).
- A worktree per chat (ADR 0019). A chat that only reads still gets a worktree.
- An agent edits main and the owner reviews. Nothing stops a bad edit.

## Decision Outcome

Chosen option: `swarm task start <name>` runs `wt switch --create swarm/<name> --base <default>`
and records the task in `~/.swarm/tasks/`. The agent edits and commits there. `swarm task land`
leaves a request in `~/.swarm/landings/`, and the swarm server runs `wt merge <default>
--no-squash --no-commit` outside the agent's sandbox, one landing at a time; Worktrunk then removes
the worktree. `swarm task drop` removes a task with no work that main lacks.

Main is read-only in layers. A hub is a repository whose git common dir is bare.

- The guard wall `main_wall` (ADR 0040) denies, for every caller, an edit tool whose file is in
  main, and a shell call that writes main by redirect, a write program, or a git write, or that
  moves the default branch (`update-ref`, `push .`, `branch -f`, `wt merge`, `core.hooksPath`).
  Shell text is read best effort.
- A Codex chat on main gets a profile where main is `read`, and main's `tmp/` and the folder of
  the task worktrees are `write`.
- A `reference-transaction` hook refuses an agent shell's update of the default branch. It is set
  only in a hub without `core.hooksPath`, and never over another tool's hook.

Main's `tmp/` stays writable, because kit runs live there and the sidebar reads them (ADR 0046).
It is ignored scratch, so it never changes main's branch. ADR 0055 is unchanged: no `wt step
prune`, and the owner's own worktrees are never removed by swarm.

### Consequences

- Good: an agent cannot change main by an edit tool on any CLI, or by any shell write in Codex.
- Bad: Claude and AGY shells are checked by text, so a path built at run time passes. Claude's
  sandbox was not turned on, because it denies the package caches a build in a task worktree
  needs. `srt` was not used, because it blocks the CLIs' own Seatbelt.
- Bad: a hub with `core.hooksPath`, such as swarm's own, gets no ref hook.
- Bad: landing needs a running swarm server. Without one, `swarm task land` says so.
- Open: pi has no swarm guard (ADR 0086), so the wall does not cover it.
- Open: sidebar groups and a chat's task badge come in a later change.

Supersedes ADR 0019 for chats in a hub's main worktree.
