---
status: accepted
date: 2026-11-23
deciders: [user]
amends: ["0149"]
related: ["0038", "0111"]
informed-by:
  - 'owner, 2026-11-23: "I know it works with Command P up and down, but that is not obvious enough [...] either allow vanilla up and down or with command up and down so that it directly switches between these projects"'
  - 'owner, 2026-11-23: "we should reuse the input box so that it shows skill previews as well", and the approved plan of that day'
---

# 0155. The new chat composer changes the project with arrows and shows the `/` menu

## Context and Problem Statement

In the composer of ADR 0149, only ⌘P changed the project: it opened a list, the arrows moved a
highlight, and Enter picked. The owner did not find that key. The prompt was a plain text area,
so a `/` showed no skill and no description, while the chat's box showed both.

## Decision Outcome

With the focus in the prompt, ⌘↑ and ⌘↓ change the project to the previous or the next one of
the ⌘P list, around its ends. Plain ↑ and ↓ do the same only while the prompt is empty; with
text they move the caret, so an edit of a long prompt cannot change the project. A change sets
the place to the new project's default, as a pick in the ⌘P list does. The hint line and the
project chip name the keys. ⌘P stays.

The prompt has the chat box's `/`, `$` and `@` menu, from one shared hook
(`web/src/react/slash-menu-state.ts`) and the same list part. The rows are those of the CLI in
the run chip. ↑ ↓ move in the open menu, Tab or Enter inserts the row, Esc closes the menu, and
⌘Enter still starts the chat. The menu opens under the prompt, because the overlay sits near the
window's top. For pi, the first prompt goes out as the chat box sends a message: a skill typed
mid-prompt moves to the start.

A new chat has no session, so two reads beside `new-preview` give the rows:

- `GET /api/chat/new-commands?cwd=&provider=` runs `swarm commands --provider <p> --json` in
  `cwd` and returns the rows of `/api/chat/commands`, with the same cache, kept per folder and
  provider.
- `GET /api/chat/new-files?cwd=` returns the rows of `/api/chat/files` for `cwd`.

Both accept only a folder in which a new chat may start, as `POST /api/chat/new` does.

## Consequences

- ⌘↑ and ⌘↓ no longer move the caret to the start or the end of this prompt.
- No account is picked before a chat starts, so the CLI lists with its default account. A
  plugin that only another account has is missing from the menu.
- With place `new`, the rows come from the project's folder, not from the worktree that the
  start makes. Both hold the same files and skills at that time.
- pi's skill chips in the text stay in the chat box only.
- A prompt that starts with a command still gives its words to the branch name (ADR 0149).
