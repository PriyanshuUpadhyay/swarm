---
status: accepted
date: 2026-10-15
deciders: [user]
amends: ["0075", "0079", "0087"]
informed-by:
  - 'owner, 2026-10-15: "Is there a command and swarm that I can use to directly spawn everything? For example, HUD starts and the server starts and it opens up the website. ... If it needs the token, that''s okay. It can be set to the local storage so that I don''t have to type it again and again."'
  - 'owner, 2026-10-15: "We can preserve a port ... have the UI show chat ID ... the route can also show /chat/chat ID"'
  - 'owner answers 2026-10-15: HUD "Start if not running"; server "Background, return at once"; port "4747"'
---

# 0088. swarm ui runs in the background on a fixed port with a lasting token

## Context and Problem Statement

`swarm ui` served in the terminal on a port the system picked, with a new token at each start, and
the page kept the token only in memory (ADR 0075, ADR 0079). A reload lost the token, a restart
changed the origin, and nothing in the URL named the open chat.

## Decision Outcome

- `swarm ui` starts `swarm ui --foreground` as a background process in its own process group, with
  its output in `<root>/ui.log` (mode 0600, because the server prints its URL with the token). It
  waits until the recorded URL answers, opens the page, and returns. `swarm ui stop` signals the
  recorded pid only while that URL still answers, so a reused pid is never signalled.
- The default port of `swarm ui` is 4747, so the page's origin and its local storage stay the same
  across restarts. A busy port is an error that names `--port`. `swarm serve` keeps port 0.
- `swarm ui` keeps one token in `<root>/ui-token` (mode 0600). `swarm serve` still makes one per
  start. The page saves the link's token in local storage and reads it on a reload; a 401 drops
  it and shows the "open the link" text. The token stays, because the server can start agents and
  type into their panes, and any site open in the browser can send requests to 127.0.0.1.
- `swarm ui` starts the usage HUD when no `SwarmHUD` process runs: the login item when it is
  installed, else the app beside this swarm. The login item stays opt-in (ADR 0087).
- The URL names the open chat as `/chat/<id>`, and the server sends the page for any `/chat/`
  path. Back and forward open the chat that the URL names. A unique start of an id opens its chat.
  The page shows the chat's short id, the shortest unique start of at least 8 characters, because
  UUID v7 ids made within about a minute share their first 8.

### Consequences

- Good: one command starts the server, the page and the HUD, and a reload keeps working.
- Bad: a token copied from local storage or `ui-token` works until that file is removed.
