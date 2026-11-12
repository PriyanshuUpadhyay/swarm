---
status: accepted
date: 2026-11-12
deciders: [user]
amends: ["0135"]
related: ["0134", "0142", "0144"]
informed-by:
  - 'owner, 2026-11-12: "It should land with a choice whether to create a PR or land on main. It should keep in mind to update the origin as well as local whenever that operation happens."'
  - 'research 2026-11-11 (~/.claude/reports/2026-11-11-worktree-landing-practice.md): local main had 52 commits origin lacked, and origin had 82 commits local main lacked'
  - 'owner decision 3a via orchestrator, 2026-11-12: start tries local sync and warns on failure; server landing requires local sync; keep the temporary Worktrunk merge target'
---

# 0145. A task lands on origin and local main together, or as a PR

## Context and Problem Statement

`swarm task land` merged only into local main. The owner also merged PRs on GitHub. Neither
path kept the two default branches in sync. New tasks started from origin under ADR 0142,
so they could lack commits from local landings.

## Decision Outcome

Run `swarm task land [<name>]` to land directly. The server fetches origin's default branch
under the existing landing lock. A failed fetch stops the operation before a merge or push.
The server fast-forwards a clean local default that is strictly behind origin. Local commits
that origin lacks stop direct landing and require reconciliation; they are never discarded.

Worktrunk merges the task into a temporary local branch at the fetched origin commit, with
`--no-squash --no-commit --no-remove --ff --verify`. This rebases the task and runs the project's
merge hooks before any push. The server pushes that commit to origin's default branch without
force. Only after origin accepts it does the server fast-forward local main and remove the
task with Worktrunk. The temporary target is deleted on success and failure. A refused push
keeps the task and does not advance main to its commit. The error says to land again.

Run `swarm task land [<name>] --pr` to push the task branch with its upstream and create a PR
into the default branch with `gh`. The title comes from the branch's commits, and the body
names the task. The result prints the PR URL. A repeated request reuses an open PR for the
same branch and base. The task and worktree stay until the owner merges and removes them.

Every task start fetches origin, including a reopen. It tries to fast-forward local main
when main is strictly behind and has no tracked changes. A dirty main, local-only commits,
or a sandbox denial gives a warning. Start continues, and a new branch uses the fetched
origin default. This sync is best effort because start runs inside the agent sandbox.
Server-side direct landing requires the sync to succeed. PR landing reports a sync warning
and still opens the PR. A later start or land brings a GitHub merge into local main.

A hub without origin keeps the local-only Worktrunk landing. `--pr` requires origin.
The CLI still waits up to 100 seconds; a server operation continues after that wait ends.

### Consequences

- Good: a successful direct land leaves origin and local main at the same commit.
- Good: the owner can choose a PR without removing its task worktree.
- Good: a refused push preserves the task and cannot leave main ahead of origin.
- Bad: existing local-only commits need reconciliation before a direct land can proceed.
- Bad: hooks that read the merge target see the temporary target rather than the default
  branch. The current repository and user Worktrunk hooks do not use that variable.
- Bad: a successful push followed by a local sync or cleanup failure leaves the task for
  recovery. The error states that origin accepted the commit. A retry can finish the sync.
- Neutral: Worktrunk's hook approval rules and the server landing lock still apply. No new
  network deadline is added; the existing server operation can outlive the CLI wait.
