---
status: accepted
date: 2026-10-01
deciders: [user]
related: ["0017", "0032", "0033"]
informed-by:
  - "User answers on 2026-10-01 (flow 01-frame): New chat starts at once with the chat profile, with no menu and no sheet; the model changes later with Switch model"
  - "Timings on the owner's Mac, swarm 0.4.1: roles check 0.51 s, models --provider codex 0.74 s, accounts about 0.5 s, each read on every New Chat open"
---

# 0035. New chat starts the chat profile with no sheet

## Context and Problem Statement

ADR 0033 made chat a profile. New Chat still opened a sheet that read the availability check,
the model list, and the accounts before Start chat was enabled, which cost about one second on
every open. The default start sends no provider or model, so it needs none of these reads.

## Considered Options

- Keep the sheet and add a fast default button.
- Start the chat profile at once from every New chat entry, and move the model pick to Switch
  model.

## Decision Outcome

Chosen: start at once. Every New chat entry, and a new or opened project or workspace, starts the
chat profile in its directory with no sheet. A pending tab shows at once, and a failure shows in
that tab with Retry. The one-off provider and model pick that ADR 0033 kept in New Chat moves to
Switch model, which opens on cached lists and runs no availability check. Chat stays a profile.

### Consequences

- Good: a chat starts with one action and no wait, and the start path makes no read that the
  launch does not need.
- Bad: a model outside the chat profile needs a start and then a switch, and the switch hands
  off a summary or recent messages, not the full conversation (ADR 0017).
