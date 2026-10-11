---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0013", "0019", "0049", "0054"]
informed-by:
  - "Owner verdicts on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-8 (b), T-6, T-7, T-8, S-7, S-8, S-9, S-29"
  - "src/store.rs:390 lists only `archived_at IS NULL`; no unarchive exists; archive leaves agents running (SwarmCLIBus.swift:132)"
---

# 0055. Close tab, End chat, Archive and Delete workspace are separate actions

## Context and Problem Statement

Close chat ended the chair and every child at once with no prompt. Archive hid a chat but left its
agents running in hidden panes. Neither could be undone: no list of archived chats and no
unarchive existed in the app or the CLI. Archiving a workspace hid its row but kept its folder,
branch and agents, and an alert from such an agent showed nowhere.

## Considered Options

- Keep close as end-and-archive and archive as hide, with no undo.
- Four actions: Close tab, End chat, Archive chat and Delete workspace, with undo for archive.

## Decision Outcome

Chosen: four actions. Close tab hides the tab (ADR 0054). End chat stops the chair and its
children and asks first when a child is live or mid-turn. Archive chat stops the agents and moves
the chat out of the sidebar; ⇧⌘T "Recently closed" and `swarm session unarchive` bring it back, and
the sessions listing can include archived chats. Delete workspace runs `git worktree remove` and
refuses a tree with uncommitted or unpushed work. Archive workspace asks when agents are live and
ends them; an agent that still waits shows its glyph on the Archived header. Clicking an archived
workspace opens it read-only; Restore stays in its menu.

### Consequences

- Good: no action stops agents by surprise, and no archive is final.
- Bad: one new CLI command and a listing flag, and Delete workspace is the first app action that
  removes files.
