---
status: accepted
date: 2026-11-17
deciders: [user]
amends: ["0135", "0142", "0145"]
related: ["0019", "0097", "0143", "0144"]
informed-by:
  - 'owner, 2026-11-17: "a deterministic way to handling these would be better… i had liked how conductor.build used to handle worktrees and their initialization without much input from the user… right now we are complicating it quite a lot"'
  - 'owner, 2026-11-17: "new chat. but we will use our own worktree setup and if we can make it leaner then we do that. user should not have to pick branch name and so on. it can be created from the first prompt of that chat. each chat can be done in a new worktree i feel"'
  - 'owner answers 1a 2a 3a 4a, 2026-11-17'
  - 'council ~/dotfiles/docs/council/2026-11-17-task-worktree-flow.md; research ~/.swarm/reports/2026-11-17-deterministic-worktree-ux.md'
---

# 0146. Each new chat gets its own worktree, named from its first prompt

## Context and Problem Statement

Under ADR 0135 a chat stayed in `wt/main`. The agent ran `swarm task start <name>` and then
worked in a second folder while its shell and the chat stayed in main. The owner found this
flow confusing and asked for one that needs no input, like Conductor's.

## Decision Outcome

A new chat whose folder is the default-branch worktree of a bare hub starts in its own new
worktree. The server makes it before the agent starts, through the New workspace path: it
fetches origin, adds `wt/chat-<8 hex>` on `swarm/chat-<8 hex>` from the origin default, and
runs the project's setup script. A failed create or setup starts no chat (ADR 0097). The
server then writes the task record and fast-forwards local main to origin (ADR 0145).

The chat's first prompt renames the branch to `swarm/` and the first five words of the prompt,
lowercase and joined by `-`, at most 40 characters. A taken name gets `-2` to `-9`. The
folder keeps its name, so the agent's folder never changes. The sidebar row takes the new name.

When a chat is archived and its branch still has no name, swarm drops its worktree. `drop`
still refuses a worktree with changes or commits. Setup output that git sees counts as a
change, so such a worktree stays.

`swarm task start` is removed. `land [--pr]`, `drop`, `list`, and `sweep` stay, and with no
name they act on the chat worktree that holds the current folder.

The "+" menu has "Default profile, no new worktree", which starts the chat in the folder
itself. Continue keeps a chat in its folder. Plain clones keep today's behavior, because the
task commands and the main wall exist only in hubs.

## Consequences

- One create path, swarm's own, makes every worktree. Worktrunk still merges and removes.
- The edit wall on main stays (ADR 0135, 0143) and points to a new chat.
- A chat that ran before this change stays in its saved folder on resume.
