---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0020", "0027", "0031", "0042", "0043", "0052"]
informed-by:
  - "Owner verdicts on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-11 (a), G-1, G-2, G-3, G-4, G-10, G-11, G-12, G-13, G-14, G-15, G-19, G-20, G-27, G-29"
  - "Owner direction 2026-10-06: one Settings window with a sidebar; one layer model (bundled default plus owner overlay, Modified, Reset to bundled)"
---

# 0057. One Settings window, and the owner's choices live in the swarm home

## Context and Problem Statement

⌘, opened nothing. Settings were spread over the Home page (profiles), a setup sheet, a Managed
Changes window, a Debug menu and a toolbar menu. Pins, names and widths lived in UserDefaults, so
they did not sync through dotfiles and branch builds shared them with the release app.

## Considered Options

- One Settings window on ⌘, and a new Home.
- Keep Home as the profiles page and add a small Settings window for the rest.
- For storage: all in UserDefaults, all in files, or view state in UserDefaults and choices in
  files.

## Decision Outcome

Chosen: one Settings window with Profiles, Skills, Accounts, Setup, Managed Changes, Appearance,
Notifications, Keys and Advanced. Advanced holds the debug toggles, "Reset declined prompts", the
data home and the helper version. Home shows recent work and, on a first run, steps to import a
project, run setup and start a chat; a project import no longer starts a chat (ADR 0059). View
state (widths, folds, selection) stays in UserDefaults. The owner's choices (row fields, tab order
and groups, pins, names, preferences) live in files in the swarm home, with the same layer model as
profiles. Profiles can be created, renamed, copied, deleted and reset to bundled, and the low-usage
threshold has a field. Setup always shows the trust choice, checks tmux, the provider CLIs, `gh`
and `yelo`, and links the bundled skills. Guard rules have a page. One split or unified setting
covers every diff view.

### Consequences

- Good: one place for every setting, and the owner's choices sync and stay apart per build.
- Bad: moving stored choices out of UserDefaults is a one-time move, and the layer model must
  cover a new file type.
