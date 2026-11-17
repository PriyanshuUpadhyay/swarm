---
status: accepted
date: 2026-11-17
deciders: [user]
amends: ["0135", "0142", "0143", "0145"]
related: ["0019", "0097", "0144"]
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
fetches origin, adds `chat-<8 hex>` in the project's worktree folder (`wt/` in a hub) on
`swarm/chat-<8 hex>` from the origin default, and runs the project's setup script. A project
with no origin default starts from the local default. With no `origin` remote, the main sync
below is skipped; with one that lacks the default branch, it gives a warning. A
failed create, setup, or launch starts no chat (ADR 0097). Swarm then archives its session and
removes the worktree and its task record through Worktrunk when the worktree holds no change.
Its branch goes too when it has no commit of its own. When the chair may
still run, as after a launch that timed out, the worktree stays. The server writes the task
record and fast-forwards local main to origin (ADR 0145). A warning from that step, such as a ref hook that could not be set, goes in the
reply and the server log.

The chat's first prompt renames the branch to `swarm/` and the first five ASCII words of the
prompt, lowercase and joined by `-`, at most 40 characters. A prompt with no ASCII word gives
`swarm/untitled-<8 hex>`. A name that a local or an origin branch has gets `-2` to `-9`; when all
are taken, the branch gets the untitled name. Origin's branches are the ones the last fetch
saw. Each new name is claimed by creating its task record, so two first prompts at once never
take one name. Only a record that names the checked-out branch is renamed. The folder keeps its
name, so the agent's folder never changes. The sidebar row takes the new name, unless the owner
renamed the row.

When a chat is archived and its branch still has no name, swarm drops its worktree, unless a
chat that is not archived still runs in it, and the app's shells in it end. `drop` still refuses a worktree with changes or
commits. Setup output that git sees counts as a change, so such a worktree stays.

`swarm task start` is removed. `land [--pr]`, `drop`, `list`, and `sweep` stay. With no name,
`land` and `drop` act on the chat worktree that holds the current folder.

On a hub's main row, the "+" menu also has "Default profile, no new worktree", which starts
the chat in the folder itself. The sidebar row carries `startsChatWorktree`, so the menu shows
that item only where a new chat gets its own worktree. The server reads the same mark from a
fresh tree, so the two agree. Continue keeps a chat in its folder. Plain clones keep today's
behavior, because the task commands and the main wall exist only in hubs.

## Consequences

- Archive and the first send wait for `wt remove` and `git branch -m` with no time limit. The
  owner accepted this as a known limit (2026-11-17); a project hook that never ends holds
  the reply.

- One create path, swarm's own, makes every worktree. Worktrunk still merges and removes.
- The edit wall on main stays (ADR 0135), and its block and its warning (ADR 0143) tell the
  agent to ask the owner for a new chat.
- A chat that ran before this change stays in its saved folder on resume.
