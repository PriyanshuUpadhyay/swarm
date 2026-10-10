---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0019", "0037", "0049"]
informed-by:
  - "Owner verdict on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-6 changed, S-20, S-21, SC-20"
  - "Superset confirms, runs git init and makes a first commit, because git worktree add needs a born HEAD (report ~/.claude/reports/2026-10-06-swarm-sidebar-customization.md)"
  - "ADR 0037 probe: an empty commit needs a git identity; two orphan worktrees have unrelated histories"
---

# 0050. Create Project makes a first commit, and an imported folder is asked once

## Context and Problem Statement

Create Project ran a bare `git init`. With no first commit, ADR 0037 made each new workspace an
orphan worktree, so the first two workspaces had unrelated histories, and step-run files in `tmp/`
showed as untracked. An imported folder with no git asked about `git init` on every "+", even
after "Keep as Folder".

## Considered Options

- Ask every time, as before.
- Always run `git init`, also on import.
- Create Project always sets up git; import asks once and remembers the answer.

## Decision Outcome

Chosen: Create Project runs `git init`, writes a `.gitignore` with `tmp/`, and makes a first commit
that holds `.gitignore`. With no git identity, it skips the commit and says so, and a workspace
falls back to the orphan worktree of ADR 0037. Importing a folder with no git asks once; "Keep as Folder" is saved
for that folder. The New Workspace sheet can start from an existing branch, a PR or a base branch,
and takes a name with no forced suffix; each project sets its worktree folder and branch prefix
(default `swarm/`) once.

### Consequences

- Good: every workspace in a new project shares one history, and `tmp/` stays out of Changes.
- Good: a folder the owner keeps plain stops asking.
- Bad: Swarm now writes a commit in the owner's repo, which needs the git identity check.
