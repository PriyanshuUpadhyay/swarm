---
status: accepted
date: 2026-11-11
deciders: [user]
amends: ["0055", "0135"]
related: ["0134", "0142"]
informed-by:
  - 'owner, 2026-11-11: decision 4a, a merge-proven cleanup sweep of every worktree in the hub, keeping branches'
  - 'council 2026-11-11 (~/dotfiles/docs/council/2026-11-11-worktree-ux.md): R5, fresh origin/default ancestry or an exact-head merged PR; R7, Git owns the inventory'
  - 'research 2026-11-11 (~/.claude/reports/2026-11-11-worktree-landing-practice.md): T3 Code cleanup, ignored-file trade-off, and the one-hour grace period'
  - 'owner clarification via orchestrator, 2026-11-12: a live session protects its cwd and task paths joined by Task.session; failed reads keep the affected worktrees'
---

# 0144. Swarm task sweep removes merged worktrees and keeps branches

## Context and Problem Statement

The hub has many worktrees that no task record names. Some hold merged work and large build
folders. `swarm task drop` knows only task records. ADR 0135 also forbids removing the owner's
worktrees. A task record cannot prove that Git still has the same checkout or that its work was
merged. A deleted upstream branch cannot prove a merge either.

## Decision Outcome

Run `swarm task sweep` to see each Git worktree's verdict, reason, and ignored-file count. Run
`swarm task sweep --apply` to remove eligible folders. The command keeps all branches. It removes
a task record only after its matching worktree has been removed. Git's worktree list is the
inventory; records add session ownership information and never authorize removal.

The command fetches origin's default branch before it checks merge proof. A failed fetch stops
the sweep before removal. The fetched default's SHA is pinned for this sweep. A worktree needs
either HEAD ancestry in that SHA, or a merged GitHub PR with the exact current HEAD SHA. A
`[gone]` upstream is no proof. Without origin, or when a needed PR lookup is unavailable or
fails, the worktree stays.

The sweep also requires an empty `git status --porcelain`, including untracked files. Ignored
files do not block removal; their count is shown. The default branch's worktree, the command's
own worktree, locked worktrees, and missing folders stay. Missing folders are left for Git prune.
A folder must be more than one hour old. The sweep reads the folder's creation time, or the
first HEAD reflog event when creation time is unavailable. Unknown age means keep.

A session is live when the adapter's pane list contains at least one of its recorded agent
panes. A live session protects its cwd's worktree and every task path that its `Task.session`
names in this hub. The sweep reads sessions even when archived. A failed store read keeps all
worktrees; a failed adapter read keeps the worktrees associated with those sessions. Closed
panes do not become live because an old hook state still says working.

Before each removal, the sweep reads Git's inventory and agent activity again and checks all
conditions. It calls `git worktree remove` without `--force`. The CLI reports removal failures
and exits with an error. Fetch, PR lookup, and adapter reads share a 120-second read budget.

### Consequences

- Good: an explicit sweep can free disk in task worktrees and owner worktrees, with branches
  available for reopening. This amends the owner-worktree restriction in ADR 0135 and the
  removal rules associated with ADR 0055. Archive keeps its existing meaning.
- Good: a new checkout at an old commit has a full hour before it can be removed.
- Bad: a branch cannot restore ignored files such as `.env`, local databases, or build output.
  The preview shows the count, and `--apply` accepts their removal.
- Bad: unmerged, dirty, active, locked, missing, or unknown worktrees keep their disk use.
- Bad: activity uses the recorded session cwd and task ownership. An agent that changes its
  folder into another worktree without updating either is not seen there.
- Bad: checks and Git removal are separate operations. A new agent or a clean branch update
  can race the final check. Git's non-force removal still checks for local file changes.
- Neutral: the sweep is a separate explicit command. It does not run on archive, on a timer,
  or as part of the pane-liveness `swarm sweep` command.
