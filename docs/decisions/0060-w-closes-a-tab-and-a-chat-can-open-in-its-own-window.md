---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0020", "0023", "0025", "0054", "0055"]
informed-by:
  - "Owner verdicts on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-14 changed, T-15, T-16, T-17, T-19, T-20 later, T-31 later, G-17, G-18, S-30 don't, GC-20, GC-34"
  - "KeyCommands.swift:138 keeps the system ⌘W Close Window; KeyRouting.swift:70 is the one key table (ADR 0023)"
---

# 0060. ⌘W closes a tab, a chat can open in its own window, and appearance is a setting

## Context and Problem Statement

⌘W closed the whole window. No key closed, reopened or flipped between chats, and ⌘9 opened the
ninth tab. A chat could not open in a second window. Appearance followed the system, and only an
environment variable changed it; text size and row height were fixed.

## Considered Options

- Keep keys, windows and appearance as before.
- Add the missing keys, "Open in New Window", and appearance settings now; a user keymap later.
- All of that plus a user keymap now.

## Decision Outcome

Chosen: the second option. ⌘W hides the tab (ADR 0054), ⇧⌘W closes the window, and ⇧⌘T reopens
from Recently closed (ADR 0055). ⌃Tab flips between recent chats. ⌘9 opens the last tab. ⌘K lists
Setup, Managed Changes and Profiles, and can close, archive, rename, reopen and switch model.
"Open in New Window" opens a chat in its own window, and windows do not share selection state.
Settings › Appearance sets System, Light or Dark, text size, density and the send key (Return or
⌘Return). The first window opens centered, not full screen. The sidebar can be up to about 560 pt
wide. Later: two chats side by side, and a user keymap once the key set settles. Rejected:
sidebar keys for pin, rename and archive.

### Consequences

- Good: keys match other tabbed Mac apps, and a chat can sit on a second monitor.
- Bad: a ⌘W from habit no longer closes the window, and two windows need separate navigation
  state.
