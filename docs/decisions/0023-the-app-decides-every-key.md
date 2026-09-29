---
status: accepted
date: 2026-09-29
deciders: [user]
related: ["0003"]
informed-by:
  - "User answer on 2026-09-29: everything goes via the app; the app decides what the terminal gets"
  - "SwiftTerm 1.19.0 MacTerminalView.swift keyDown (464df52)"
---

# 0023. The app decides every key

In the context of keyboard-first navigation over live SwiftTerm panes, facing SwiftTerm taking
every ⌘ key in `keyDown` and silently dropping unhandled ones (and ⌥⌘O flipping Option-as-Meta),
we chose to make every app action a menu command and to have the terminal view pass a key to
SwiftTerm only when `KeyRouting.route` says so, and neglected leaving keys to the terminal and an
app-wide event monitor, to get one tested owner for key decisions, accepting that a key the
router does not list never reaches the agent until the router adds it.
