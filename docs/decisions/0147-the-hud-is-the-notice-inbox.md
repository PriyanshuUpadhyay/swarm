---
status: accepted
date: 2026-11-18
deciders: [user]
amends: ["0077", "0138"]
related: ["0087", "0102"]
informed-by:
  - 'owner, 2026-11-18: "we can get rid of the mac os banner if we notify via hud"'
  - 'owner, 2026-11-18: "a chat is working and the HUD is closed; the chat ends its turn with a question; I see a popus showing that the chat is done with the start of the message text with a button click that takes me to the chat."'
  - 'owner, 2026-11-18: "vibeisland is good enough" (the design target); answers 2a 3a, numbers "for now yeah"'
  - 'research ~/.swarm/reports/2026-11-18-agent-notifications.md'
---

# 0147. The HUD is the notice inbox

## Context and Problem Statement

A notice said `Swarm` and `<folder>: the orchestrator is done`. With many chats open, the owner
could not tell which chat ended or what it said, and a banner that was missed was gone. The notch
HUD (ADR 0087) is always on screen and already polls the store every 3 s.

## Decision Outcome

**A notice names the chat and quotes it.** The hook stores the chat's title (the chair log's
title, else the workspace folder) and the first sentence of the chair's last reply, with a kind:
`done`, `asks` when the reply ends with a question, `waiting`, or `failed` for a turn that
stopped on an error (Claude `StopFailure`). A `done` or `asks` turn under 30 s posts nothing; a
wait or a chair turn that failed posts at any turn length, because the owner has to act on it.
A turn that the owner stopped, such as a Codex `Interrupt`, posts nothing, because the owner was
there.

**The store keeps an inbox.** `notice.seen_at` marks a row the owner has seen. A row is seen when
the owner opens the chat in the app (the `selected_chat` or `seen` choice), clicks Open on the
row in the HUD, or clicks Clear. Open marks nothing seen when `swarm focus` fails.
`swarm notices --inbox --json` (`swarm.inbox.v1`) gives up to 3 blocked agents with the prompt on
their screen, and the newest unseen row of up to 20 chats, with the full count of each. A
blocked agent stays while its pane is open, because a block sends no new state reports.
`swarm notices seen <session>...` marks rows seen. Clear names the chats it shows, so a row that
came after the HUD drew stays.

**The HUD shows it.** The closed wing says `needs you · <title>`, else `stopped`, else `N done`.
A new item drops a popup out of the notch with the title, the kind, the reply's first sentence,
and Open. Open runs `swarm focus` and marks the chat seen; when the focus fails, the popup line
shows why for 4 s. A done popup closes after 4 s; a question, a stop, or a wait stays. A hover
on the notch row opens the card, which takes the popup's place. Items within 10 s join one popup. The open card's Inbox
section lists up to 2 blocked agents, with a button per choice that runs `swarm answer` when the
prompt has at most 4 choices and Open alone otherwise, and the unseen chats with Clear. The HUD
has no text field, so to reply the owner clicks Open.

**No banner while the HUD runs.** `swarm notices --due` gives no banner for a chat's row while the
HUD's login item runs. A row with no chat, such as the Settings test notice or a `swarm notify`
from a shell, has no place in the HUD, so it keeps its banner. With no HUD the macOS banner stays
as ADR 0077 says: its text now holds the chat title and the quoted reply, and its style does not
change. The Settings `done` switch mutes only `done` banners; an `asks` or `failed` row and the
Settings test notice (kind `test`) still post. The hook reads the HUD's state with a 2 s limit,
and past it the banner posts.

### Consequences

- Good: the owner sees which chat ended and what it said, and a missed popup stays in the inbox.
- Good: the HUD answers a permission through `swarm answer`, which already refuses a stale prompt.
- Bad: the inbox read costs up to 3 screen reads every 3 s while agents wait.
- Bad: a chat whose title the log does not hold shows its folder name.
