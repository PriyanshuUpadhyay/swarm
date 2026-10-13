---
status: accepted
date: 2026-10-07
deciders: [user]
supersedes: "0035"
related: ["0017", "0032", "0033", "0049"]
informed-by:
  - "Owner verdicts on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-13 (b), S-23, S-24, G-5"
  - "SwarmChatLaunch.swift:44 fixes role `chat`; SwarmApp.swift:1243 and :676 start a chat on every import or create"
---

# 0059. "+" starts a chat in one click, "New chat as…" picks a profile, and adding a project starts nothing

## Context and Problem Statement

ADR 0035 made every New chat start the chat profile at once, and made a new or imported project or
workspace start one too. A chat on `lane`, Codex or `code.complex` needed a start and then a
switch, which hands off only a summary. Importing three projects to set up the sidebar started
three agents.

## Considered Options

- Always the chat profile, as before.
- One click starts the chat profile, and a "New chat as…" menu lists every profile.
- A sheet on every new chat.

## Decision Outcome

Chosen: a click on "+" starts the chat profile at once with no sheet, as ADR 0035 did. A long press
or right-click on "+" opens "New chat as…" with the profile list. Adding a project or workspace
starts no agent; an empty workspace shows New chat (ADR 0049). Kept from ADR 0035: the pending tab,
Retry on failure, and Switch model on cached lists.

### Consequences

- Good: the one-click start stays, and any profile starts without a handoff.
- Bad: a new project needs one more click before an agent runs.
