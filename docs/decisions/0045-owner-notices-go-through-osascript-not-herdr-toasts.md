---
status: superseded
superseded-by: "0058"
date: 2026-10-05
deciders: [user]
related: ["0036", "0044"]
informed-by:
  - "Owner answer Q1 (a) on 2026-10-05, flow tmp/flow/2026-10-05-swarm-notify (03-contracts.md)"
  - "Council 2026-09-24 notify plan (docs/council/2026-09-24-roadmap-notes.md:104), which named Herdr `notification show`; this record replaces that path"
  - "herdr api schema --json (herdr 0.9.2-preview): NotificationShowReason has `disabled`; the owner's ~/.config/herdr/config.toml has [ui.toast] delivery = \"off\""
---

# 0045. Owner notices go through osascript, not Herdr toasts

## Context and Problem Statement

Swarm needs one way to show the owner a notice on every adapter. Herdr has
`herdr notification show`, but it most likely obeys `[ui.toast] delivery`, the same switch that
controls Herdr's own state toasts. The owner keeps those toasts and Herdr's sound off, so Herdr
would either drop swarm's notice or show each state change twice.

## Considered Options

- Every macOS adapter's `notify` verb runs osascript `display notification`, Herdr included; swarm's
  Herdr switch turns Herdr's own toast and sound off.
- The Herdr adapter runs `herdr notification show` and checks the reply for `"shown":true`.
- Herdr first, and osascript when Herdr reports the notice not shown.

## Decision Outcome

Chosen: osascript on every macOS adapter. The verb passes the title and body to osascript as
`argv`, so agent-written text never becomes AppleScript source. The Herdr switch that turns
Herdr's own toast and sound off is a consented, revertible write through the managed-edits module
(ADR 0036). Rejected: Herdr `notification show`, because it needs Herdr toasts on, so Herdr's own
toasts repeat swarm's; Herdr first with a fallback, because it is two paths and still repeats when
Herdr toasts are on.

### Consequences

- Good: one path on every adapter, and swarm's notice shows while Herdr's own toasts stay off.
- Good: a machine can override the one `notify` line in a deployed adapter file.
- Bad: a notice has no Herdr click-to-jump to the pane.
- Bad: the notice shows "Script Editor" as its app, and a fresh Mac asks once for notice permission.
