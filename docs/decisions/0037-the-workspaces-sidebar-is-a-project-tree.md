---
status: superseded
superseded-by: "0049"
date: 2026-10-01
deciders: [user]
related: ["0019", "0020", "0023", "0035"]
informed-by:
  - "User answers on 2026-10-01 (flow 01-frame): Conductor's project tree; the workspace \"+\" adds a chat tab; ⌘N for workspaces, ⇧⌘N for projects; a plain folder gets a git init offer"
  - "User answer on 2026-10-01 (flow 03-contracts): the first workspace in a repo with no commit is an orphan worktree"
  - "Probe on Apple Git 2.54.0, fresh HOME: `git worktree add -b <b> <path> refs/heads/master` in a repo with no commit fails with `invalid reference`; `--orphan -b` works; an empty commit needs a git identity"
---

# 0037. The Workspaces sidebar is a project tree

## Context and Problem Statement

The Workspaces view listed every workspace in one flat list, sorted by activity, and showed the
project only as a "project / folder" title prefix. The one "+" opened a sheet that asked for a
project and then a name. ⌘N started a chat. Create Project made a folder with no git, so it could
get no workspace, and ADR 0019 left plain folders with no Git setup.

## Considered Options

- Keep the flat list and add a project filter.
- A project tree like Conductor's: a header per project, and a "+" at each level.
- For a repo with no commit: an empty first commit, an orphan worktree, or the main checkout as
  the first workspace.

## Decision Outcome

Chosen: the project tree. Pinned stays a section at the top. Then each project has a collapsible
header, in path order, with its workspaces under it in last-activity order; a status change never
moves a row. The top "+" makes or imports a project (⇧⌘N). A header's "+" makes a workspace there
(⌘N, in the current project). A row's "+" adds a chat tab through New chat (⌘T, ADR 0035). Chats
stay in the tab strip and do not become rows. Create Project runs `git init`. Import and a plain
project's "+" offer `git init`; "Keep as Folder" keeps the plain project of ADR 0019. In a repo
with no commit, a new workspace is an orphan worktree (`git worktree add --orphan -b`), so Swarm
makes no commit and needs no git identity.

### Consequences

- Good: each workspace shows under its repository, and each level has one action for its next
  step.
- Good: a new project can get a workspace at once on a Mac with no git identity.
- Bad: ⌘N changes meaning. A press from habit opens the workspace sheet, which Escape closes and
  which tells the owner that ⌘T adds a chat.
- Bad: two workspaces made before the first commit have unrelated histories, so the second needs
  `--allow-unrelated-histories` to merge.
- Bad: recent work in a lower project can be off screen; a collapsed header shows its most urgent
  status for this.
