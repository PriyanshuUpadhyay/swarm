---
status: accepted
date: 2026-10-19
deciders: [user]
related: ["0054", "0085", "0093", "0096"]
informed-by:
  - 'owner, 2026-10-20: accepted at tab-sync close ("accept all")'
  - 'owner, 2026-10-19: approved plan tmp/plan/2026-10-19-tab-sync/plan.md, decisions 3 and 4 ("an ended tab-only chat keeps its tab until the owner closes it")'
  - src/view/choices.rs:464 (ChoiceOp::SelectedChat closes the old selection's tab when is_running is Some(false))
  - src/view/tree.rs:676 and src/serve.rs:3734 (stored agents carry alive None, which made is_running Some(false) for a live chat)
---

# 0098. An ended tab-only chat keeps its tab until the owner closes it

## Context and Problem Statement

ADR 0054 lets an ended chat's tab hide when the owner leaves it. ADR 0085 says a chat started in a
tab shows only as a tab, and closing its tab archives it. The two rules meet on an ended tab-only
chat. Selecting another chat closes its tab with no archive, so by ADR 0093 the chat comes back as a
sidebar row that the owner never asked for. The same path fired for a live chat when one pane list
failed, because its stored agents have no `alive` value and the chat read as ended. This is the
path that ADR 0093 could not find.

## Considered Options

- An ended tab-only chat keeps its tab; only the owner closes it, and that close archives it.
- Leaving an ended tab-only chat archives it, so the hide rule and the archive rule both hold.
- Keep ADR 0054 as written for every chat, and accept the sidebar row that ADR 0093 then shows.

## Decision Outcome

Chosen: the first option, because a tab-only chat lives only in its tab, and an archive that the
owner did not ask for would stop it with no confirm. The hide rule of ADR 0054 stays for chats that
the sidebar lists. A tab also hides only when every agent of the chat is confirmed ended; an agent
whose liveness is unknown does not count as ended.

### Consequences

- Good: a tab-only chat never turns into a sidebar row by itself, and a live chat never loses its
  tab when one pane list fails.
- Good: the strip, the sidebar, and the archive list agree on where each tab-only chat is.
- Bad: ended tab-only chats stay in the strip until the owner closes them, so a long session can
  collect ended tabs.
- Bad: two tab rules now depend on the `tab_chats` mark, so a chat that loses that mark goes back
  to the ADR 0054 rule.
