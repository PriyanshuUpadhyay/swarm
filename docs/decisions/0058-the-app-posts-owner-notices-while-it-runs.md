---
status: accepted
date: 2026-10-07
deciders: [user]
supersedes: "0045"
related: ["0036", "0044"]
informed-by:
  - "Owner verdicts on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-12 (b), dock badge yes, G-16, S-28, GC-27"
  - "ADR 0045 consequences: no click-to-jump, and the notice shows Script Editor as its app"
---

# 0058. The app posts owner notices while it runs, and osascript covers the rest

## Context and Problem Statement

ADR 0045 sent every owner notice through osascript. The notice showed "Script Editor" as its app,
played "Glass", and a click did not open the chat. The owner could not mute a project, change the
sound or turn off done notices, and the dock had no badge.

## Considered Options

- osascript only, as before.
- The app posts through `UNUserNotificationCenter` while it runs, and `swarm notify` uses osascript
  only when the app does not run.
- Both, with a setting.

## Decision Outcome

Chosen: the app posts while it runs. Its notice carries the app's name, and a click selects the
chat. Settings › Notifications sets a mute for each project, the sound, and whether done notices
show. The dock badge counts chats that need input and has an off switch. When the app does not run,
`swarm notify` keeps the osascript path of ADR 0045, with `argv` passing, so agent text never
becomes AppleScript source. The Herdr toast switch of ADR 0045 stays. ADR 0044 still decides when a
notice fires.

### Consequences

- Good: click-to-jump, the right app name, and per-project control.
- Bad: two paths; `swarm notify` must know whether the app runs, or the owner gets a notice twice.
