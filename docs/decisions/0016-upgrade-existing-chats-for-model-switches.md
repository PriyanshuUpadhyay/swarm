---
status: accepted
date: 2026-09-24
deciders: [user]
supersedes: 0015
superseded-by:
related: [0005, 0008, 0013]
informed-by:
  - docs/decisions/0015-one-schema-file-uuid-v7-sessions-and-a-home-per-branch.md
  - migrations/0001.sql
  - src/store.rs
links: []
---

# 0016. Upgrade existing chats for model switches

## Context and Problem Statement

Switching a chat from Claude to Codex starts a new provider session. The app must keep both
transcripts in one sidebar chat and must keep existing chats during installation. The single-schema
rule in ADR-0015 rejects upgrades, so it cannot store the link in an existing database.

## Considered Options

- Add a numbered database migration and save the link in Swarm's session table.
- Save the link only in the Mac app, outside Swarm's session database.
- Show the new provider session as a separate sidebar chat.

## Decision Outcome

We chose a numbered migration. Swarm stores `continuation_of` on the new session, upgrades a
version-1 database in place, and still rejects unknown schema versions. UUID v7 session ids,
per-branch homes, and the other session rules from ADR-0015 stay in force. This record replaces
ADR-0015's one-schema and no-upgrade rule.

## Decision Drivers

- The user wants a model switch to remain one chat and carry a compact summary.
- The user wants existing chats to survive installation.
- The CLI, app, and database must agree on chat membership.

## Pros and Cons of the Options

### Numbered database migration

- Good: links are available to every Swarm client and survive app reinstall.
- Bad: schema upgrades and branch builds now need migration care.

### Mac app storage

- Good: the Swarm database remains unchanged.
- Bad: other clients cannot see the link, and app data loss splits the chat.

### Separate sidebar chat

- Good: no new saved state is needed.
- Bad: a model switch breaks the visible conversation.

## Confirmation

Tests upgrade a version-1 database, reject invalid links, and group both provider sessions in one
sidebar chat. A live switch still needs a manual UI check.

## Informed by

- ADR-0015 and the existing session schema.

## Links

- Supersedes ADR-0015.
