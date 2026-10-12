---
status: proposed
date: 2026-10-12
deciders: [user]
related: ["0006", "0057", "0069", "0072"]
informed-by:
  - 'owner, 2026-10-12: "even the usage hud install prompt in the app so that it can be added"'
  - "~/dotfiles/docs/council/2026-10-12-swarm-owns-accounts-isolation.md"
  - "yelo apps/UsageHUD, the app this panel is ported from"
---

# 0073. The usage HUD is a panel inside the app

## Context and Problem Statement

Yelo ships the usage HUD as a second app in `~/Applications`, started by a LaunchAgent. The owner
wants the HUD in swarm, added from a prompt in the app.

## Considered Options

- A panel inside Swarm.app.
- A helper app inside the bundle, registered as a login item.

## Decision Outcome

Chosen: a panel inside Swarm.app. The app already starts at login and keeps running with no
window (ADR 0069), so the panel needs no second process. The panel is off until the owner accepts
one prompt, "Add the usage HUD?". The answer is the `usageHUD` choice in `choices.json`, and a
switch in Settings › Accounts changes it later. History is `root_dir/usage-history.json`. Panel
sizes are view state in UserDefaults (ADR 0057). The panel reads `swarm usage --json` only.

The HUD adds no menu-bar item, so ADR 0069 stands. A Mac with no notch gets a small pill at the
top of the screen.

Rejected: the helper app, because it needs a second bundle, a login registration and its own
signing check, for the same panel.

## Consequences

- Good: nothing is installed outside the app, and removing the HUD is one switch.
- Bad: the HUD shows only while Swarm.app runs.
