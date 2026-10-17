---
status: accepted
date: 2026-10-15
deciders: [user]
supersedes: "0073"
amends: ["0076", "0077"]
related: ["0057", "0069", "0074", "0079"]
informed-by:
  - 'owner, 2026-10-15: "we can build a similar notch along with the usage metrics that we have. If it is not the notch one, we can think of our own notch build."'
  - 'owner answers 2026-10-15: host "Port yelo HUD"; start "Login item via CLI"'
  - 'owner, 2026-10-15: "we need a sweet S redesign of it ... get the UI reviewed by Codex Seat ... different design languages. Pick one ... different themes within the HUD that change based on time of the day"'
  - "yelo apps/UsageHUD at f14e0f5, the owner's own notch HUD, ported here"
  - "tmp/flow/2026-10-15-notch-hud/02-design.md (notch app survey, three looks, Codex review)"
---

# 0087. The usage HUD is a notch helper beside swarm, started at login on opt-in

## Context and Problem Statement

ADR 0073 put the usage HUD in Swarm.app, and ADR 0076 dropped it with the SwiftUI app. ADR 0079
then removed the app window: `swarm ui` opens a browser tab. The owner wants the notch HUD back,
redesigned, with themes that follow the time of day. Nothing native is left to host it.

## Considered Options

- Port yelo's `apps/UsageHUD` as a helper app in `libexec`, beside `SwarmNotify.app`.
- Write a new notch helper from scratch.
- Keep only the compact meters in the web UI.

## Decision Outcome

Chosen: port yelo's HUD as `libexec/SwarmHUD.app`, because its window code already handles the
notch, hover, click-through and the lock-screen wedge, and only its look and data model change.

- The HUD reads `swarm usage --json` (ADR 0074) and refreshes with
  `swarm usage --refresh --provider <p> --json`, by hand only, at most once per provider in 300 s.
  It finds `swarm` through `SWARM_BIN`, which the LaunchAgent sets with the installer's `PATH`.
- It opens on hover, and ⌃⌥⌘U or a VoiceOver action opens it for the keyboard. Both use APIs
  that need no input-monitoring permission.
- `swarm hud install` writes a LaunchAgent, so the HUD starts at login. `swarm hud uninstall`
  removes it, and `swarm hud status --json` prints `swarm.hud.v1`. This amends ADR 0077 ("nothing
  starts at login"): the HUD starts at login only after the owner opts in.
- The web UI asks "Add the usage HUD?" once (pref `usageHUDAsked`), and Settings › Accounts has a
  switch. The switch shows the install state, which only the LaunchAgent holds. This amends the
  "dropped" line of ADR 0076.
- The look is one design language, chosen from three candidates after a Codex seat reviewed them
  (`02-design.md`). The theme follows the local hour, and the right-click menu can pin one theme.
- No menu-bar item, so ADR 0069 stands. Quit is in the right-click menu.

Rejected: a new helper from scratch, because it repeats the window work yelo already proved; the
web-only meters, because the owner asked for the notch.

## Consequences

- Good: the HUD runs with no browser tab and no `swarm ui` server.
- Good: `brew upgrade` keeps the LaunchAgent valid, because it names the `opt/swarm` path.
- Bad: one more process at login for an owner who opts in.
- Bad: one LaunchAgent label, so a dev build's `swarm hud install` replaces the release one.
- Bad: the HUD needs macOS 26 on arm64, as the formula does (ADR 0079).

## Links

- Supersedes: 0073. Amends: 0076, 0077. Related: 0057, 0069, 0074, 0079.

## Amendment 2026-10-17: Renew refreshes once

Deciders: [user]. Informed by the owner request "It used to give me an option to renew ... give me
that option again", and by the yelo HUD, which had the same Renew flow.

A Claude row with an expired token has a Renew link. It runs
`swarm run claude --account <name> -- -p /usage` in the background, with a 60 s limit, and then
refreshes that provider one time, even inside the 300 s limit. That refresh is the point of Renew,
because only a new reading shows that the token works again. Other refreshes keep the limit.
