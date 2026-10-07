---
status: accepted
date: 2026-10-07
deciders: [user]
supersedes: "0037"
related: ["0011", "0019", "0020", "0023", "0035"]
informed-by:
  - "Owner verdicts on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-1 (b), D-2 changed to (a), D-5 (c), S-11, S-19, S-27, S-31, S-25"
  - "Audit and research, ~/.claude/reports/2026-10-06-swarm-sidebar-customization.md: Zed, Codex, VS Code, Conductor and Superset list chats under the project; Codex lists projects with no chats"
  - "SessionsTree.swift:151 and :182 drop a project that came from a CLI session when its last chat is archived"
---

# 0049. Chats are rows under their workspace, and a project stays until it is removed

## Context and Problem Statement

ADR 0037 made the sidebar a project › workspace tree and kept chats in the tab strip. The owner
could not find "the login chat" without opening each workspace, because a row showed only the
folder, the branch and "N chats". A project that came from a terminal `swarm launch` left the
sidebar when its last chat was archived, so its empty worktrees had no "+" to start a chat. A
one-off launch in `~/Downloads` added a project that could not be removed. Rows sorted by last bus
message, so a row could move under the pointer.

## Considered Options

- Keep workspace rows and show the newest chat title as a subtitle.
- Project › workspace › chat rows, with child agents folded under their chat.
- A setting that switches between the two.
- For grouping: project only, or a one-key toggle to a group-by-state view, or a flat list.
- For which projects show: imported ones only, ones with a live chat (as before), or every project
  seen once until the owner removes it.

## Decision Outcome

Chosen: chat rows under their workspace, grouped by project only. Each project and workspace folds,
and a folded row shows the most urgent state inside it. Child agents fold under their chat. Projects
keep path order; the owner orders and pins workspaces by drag, and chats inside a workspace stay
newest first. A project is saved the first time a session appears in it and stays with all its
worktrees, even with no chats, until "Remove project" writes an ignore entry. The owner can rename a
project. A worktree whose folder is gone shows "folder missing", disables New chat and offers
"Prune worktree". Chats of a removed worktree fold under one "Removed worktrees" row per project.
Per-path names, pins and archive flags are pruned when their path leaves the list. Kept from ADR
0037: Pinned at the top, the top "+" (⇧⌘N), a header "+" for a workspace (⌘N) and a row "+" for a
chat (⌘T). Rejected: a group-by-state toggle, because the rolled-up state on folded rows already
shows urgency; a sidebar filter field, because ⌘K is the search.

### Consequences

- Good: every chat is one glance away, and an empty workspace can start a chat.
- Good: rows move only when the owner moves them.
- Bad: a workspace with many chats makes a long list, so old chats need a "show more" cut.
- Bad: the tab strip no longer lists every chat, so ADR 0054 must define what a tab is.
