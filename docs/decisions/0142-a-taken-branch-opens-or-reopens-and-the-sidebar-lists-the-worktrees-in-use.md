---
status: accepted
date: 2026-11-11
deciders: [user]
amends: ["0135", "0055"]
related: ["0049", "0135", "0055"]
informed-by:
  - 'owner, 2026-11-11: "loads of UX issues around creating, managing, deleting, opening an existing work tree using the plus sign"; chose plan A of the council'
  - 'council 2026-11-11 (~/dotfiles/docs/council/2026-11-11-worktree-ux.md): GO-WITH-CHANGES, unanimous at round 3'
  - 'research 2026-11-11 (~/.swarm/reports/2026-11-11-worktree-landing-practice.md)'
---

# 0142. A taken branch opens or reopens, and the sidebar lists the worktrees in use

## Context and Problem Statement

The sidebar "+" and `swarm task start` both failed on a branch that already existed. "+" showed
git's text "a branch named … already exists", and `swarm task start` showed Worktrunk's "Branch …
already exists". Remove keeps the branch, so each removal left one more name that failed. "+" >
Existing branch hid every branch that had a worktree, so a worktree could not be opened from "+".
New work started from the local default branch, which can lag `origin`. An opened hub listed all
of its worktrees, 34 rows in the swarm hub, most of them agent task worktrees with no chat.

## Decision Outcome

- One rule, `task::branch_state`, tells both create paths whether a branch is absent, free, or
  checked out, and where.
  - "+" New branch on a taken name makes nothing. The dialog shows "Open workspace" for a branch
    with a worktree, or "Reopen branch" for a branch with none.
  - "+" Existing branch lists the checked-out branches first, marked "open"; a pick selects that
    worktree with no git write, no setup script, and no new chat. The default branch is not in
    the list, because main is always in the sidebar.
  - `swarm task start x` reopens a left-over `swarm/x` with `wt switch` and no `--create`. It
    returns the task when this session's record names its worktree, and refuses with the path
    when another worktree holds it.
- A new branch starts from `origin/<default>` after a fetch, when origin has that branch, with no
  upstream. A failed fetch makes nothing. Without origin, the local default branch is the base.
  Reopen keeps the branch's own commit.
- "+" stays on git, because it serves plain clones and a chosen folder. `swarm task` stays on
  Worktrunk.
- An opened project lists main, a worktree with a chat, a missing one, and one the owner made,
  opened, pinned, or archived. The rest are one "N more worktrees" row, which opens "+" on
  Existing branch. Swarm adds no new record; git's worktree list stays the inventory.
- Delete workspace is now "Remove worktree…". It removes the folder and keeps the branch.
  Archive keeps its meaning (ADR 0055) and says that the folder stays on disk.

### Consequences

- Good: no create path fails on a left-over branch, and every worktree can be opened from "+".
- Good: the sidebar shows the worktrees the owner uses, and the count shows what waits.
- Bad: a new branch from `origin/<default>` lacks commits that are only on the local default
  branch, until they are pushed.
- Bad: a hidden worktree still uses disk. The sweep of ADR 0135's 4a cleanup is not in this
  change.
- Open: task worktrees under their chat (ADR 0135 "Open"), and "+" on Worktrunk, come later.
