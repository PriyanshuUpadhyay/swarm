---
status: accepted
date: 2026-10-19
deciders: [user]
related: []
informed-by:
  - 'owner, 2026-10-19 (paraphrase): when a workspace is created it should by default start a chat so that I can start typing; the whole point of creating a workspace is to start a new chat'
  - web/src/core/lifecycle.ts (Lifecycle.create)
---

# 0095. A new workspace starts a chat

## Context and Problem Statement

After the New workspace dialog created a workspace, the app selected it and stopped there. Only a
workspace made from an issue started a chat. The owner then had to press ⌘T before typing, while
the reason to create a workspace is to start a chat in it.

## Considered Options

- Each new workspace starts a chat in the default profile, with the issue in its draft when the
  workspace is made from an issue.
- Only a workspace made from an issue starts a chat.

## Decision Outcome

Chosen: the first option, because it removes a step from the most common path.

- `Lifecycle.create` starts a chat after it selects the new workspace.
- A create whose dialog the owner closed first, or whose setup script failed, starts no chat. It
  shows a notice or keeps the dialog, as before, so the app does not move the owner.

### Consequences

- Good: the owner can type in the new workspace at once.
- Bad: each new workspace starts an agent, also when the owner only wanted the checkout.
