---
status: accepted
date: 2026-10-23
deciders: [user]
related: ["0115", "0097"]
informed-by:
  - 'owner, 2026-10-23 (paraphrase): if a project is not a git folder we cannot add workspace branches, so just list only the chats that are created there'
  - 'owner screenshot, 2026-10-23: the "+" of the plain folder "Monthly topics" opened New workspace, which said "New Workspace needs a Git project"'
---

# 0116. A plain folder project shows one row with its chats

## Context and Problem Statement

A project that is not a Git repository has one workspace, the folder itself, and no branches. The
sidebar still drew it as a project header, then a workspace row with the same name, then its
chats. The header's "+" opened New workspace, which can only fail for a folder.

## Considered Options

- Drop the header, and let the folder's one workspace row stand for the project (chosen).
- Drop the workspace row, and give the header the workspace's click, "+" and menu.
- Keep both rows, and only disable New workspace for a folder.

## Decision Outcome

Chosen: the first option, because the workspace row already opens the workspace, starts a chat
with "+", pins, renames, archives and copies its path. A header in its place would need each of
those again.

- A project section is a folder when the project has no Git common directory, has one workspace,
  and lists that workspace's row. The tree sends `folder: true` for it.
- The app draws no header for a folder. Its workspace row takes the header's look and letter
  avatar, and shows the project's name, so Rename Project still shows. Its chats and the
  "Archived · N" fold sit under it.
- The row's menu adds Remove Project…, which the header carried.
- New Workspace (⌘N, the dialog's project list) offers only Git projects. With none, a notice says
  that New Workspace needs a Git project.
- The archive view, and a folder whose workspace is pinned, keep the header.

### Consequences

- Good: a folder reads as one line and its chats, and no control leads to an error.
- Good: the workspace row's actions and keys work as before.
- Bad: a Git project and a folder project look alike at the top line, and differ only in the rows
  under them.
