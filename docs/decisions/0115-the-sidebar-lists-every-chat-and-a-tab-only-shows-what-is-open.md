---
status: accepted
date: 2026-10-23
deciders: [user]
supersedes: ["0093", "0096"]
amends: ["0085", "0098"]
related: ["0054"]
informed-by:
  - 'owner, 2026-10-23 (paraphrase): even if a tab is inside a workspace, sometimes it shows up in the sidebar and sometimes it does not; it is very confusing'
  - 'owner screenshot, 2026-10-23: one workspace had 3 open tabs and listed 2 chats; the third was a tab chat that was not the first tab'
  - src/view/tree.rs (Workspace::listed_chats before this change)
---

# 0115. The sidebar lists every chat, and a tab only shows what is open

## Context and Problem Statement

ADR 0085 kept a chat started with ⌘T or the tab strip's "+" out of the sidebar while its tab was
open, and closing its tab archived it. ADR 0093 listed such a chat again when its tab left the
strip, ADR 0096 always listed the first open tab, and ADR 0098 kept an ended tab chat's tab. So
whether a chat had a sidebar row depended on how it started, whether its tab was open, and the tab
order. The owner could not predict where a chat would be.

## Considered Options

- The sidebar lists every chat of a workspace, and the tab strip shows the open ones (chosen).
- Keep the tab-only rule and its three exceptions.
- The sidebar lists workspaces only, and chats live only in tabs.

## Decision Outcome

Chosen: the first option, as an editor lists every file in its tree and the open ones as tabs.
One rule then decides every row.

- The tree lists every listed chat of a workspace (ADR 0084 still decides which chats are
  listed). `Workspace::listed_chats` and the `tab_chats` choice are gone.
- ⌘T and the strip's "+" start a plain chat. `open_tab` has no `tab_only` field.
- Closing a tab only closes the tab. It does not end or archive the chat, and ⇧⌘T opens it again.
- Leaving an ended chat closes its tab for every chat, as ADR 0054's strip does. The exception
  that ADR 0098 made for a tab-only chat is gone; its rule that an agent of unknown liveness does
  not count as ended stays.
- The import half of ADR 0085 stays.

### Consequences

- Good: a chat's row does not depend on its tab, so the row count, status and badges of a
  workspace match its chats.
- Good: a live chat is never invisible, which removes the case that ADR 0093 patched.
- Bad: a workspace with many tabs has as many rows. The "… N more" fold after 5 chats limits it.
- Bad: a `tab_chats` key in an existing choices file stays as an unknown key until the file is
  written without it.
