---
status: accepted
date: 2026-11-23
deciders: [user]
amends: ["0147", "0087"]
related: ["0138"]
informed-by:
  - 'owner, 2026-11-23: "we can use HUD to show the list of chats that are active right now, and when we click on it, we just get taken there"'
  - 'owner, 2026-11-23: "a notification sound, something unique, to tell us that [...] the whole chat is done, not just the orchestrator"'
  - 'owner, 2026-11-23: "we can maybe make it a listener and publisher kind of a thing so that we are not polling every single time", then "A, 1"'
---

# 0156. The HUD lists active chats, chimes when a chat converges, and reads on a change signal

## Context and Problem Statement

The HUD's inbox (ADR 0147) keeps a chat's newest unseen result. A chat whose chair ended a turn
earlier, and whose new child seat now works, still showed `1 done`, so the owner could not tell
that the chat was still busy. While the HUD runs, a chat's notice posts no banner, so it played no
sound. The HUD read `swarm working --json` and `swarm notices --inbox --json` every 3 s, two process
starts each time.

## Decision Outcome

- `swarm notices --inbox --json` adds `active`, one entry per chat with a working or waiting agent
  (`session`, `title`, `working`, `blocked`; waits first, at most 10), `active_total`, and `sound`,
  the Settings switch `notices.sound`. The schema stays `swarm.inbox.v1`; an older HUD ignores the
  keys.
- The open card shows "Active chats" under the inbox, at most 5 rows. A row opens its chat through
  `swarm focus`, as an inbox row does.
- A new `done` or `asks` row means that the chair ended its turn while no child worked (ADR 0138),
  so the chat converged. The HUD then plays the macOS system sound "Glass", once per read, while
  `sound` is on. The first read after a start plays nothing.
- A store write that the HUD shows posts the Darwin notify name
  `io.github.priyanshuupadhyay.swarm.changed` (`hud::signal_changed`): a change of agent state, a
  pane set or cleared, a notice added, and notices marked seen. A `working` report that repeats the
  state posts nothing. The HUD reads both lists 250 ms after a signal, so a burst costs one read,
  and a signal during a read runs the read again after it.
- The 3 s poll becomes 60 s. It still catches what no write marks: a `working` report that grows
  older than 10 minutes, and a lost signal.

## Consequences

- A chat shows as active while any agent in it works, so a stale `done` row no longer hides it.
- With the 9-account fixture, the card reaches its 660 pt cap and the account rows scroll.
- The signal has no data and no home, so a dev build's store write also makes the owner's HUD read
  again. That read finds nothing new and costs one read.
- A failed `active` title read fails the whole inbox read, as a failed `needs` title read does.
