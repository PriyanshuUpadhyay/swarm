---
status: accepted
date: 2026-10-26
deciders: [user]
amends: ["0125"]
related: ["0087"]
informed-by:
  - 'owner, 2026-10-26: "yes" to build the reload, with the next release as its proof'
  - "log of 0.0.45: each new brew build failed its first start with `Launch Constraint Violation c[5]p[1]m[1]e[0]`; launchd logged `Requesting LWCR update on next spawn`, and the next try 60 s later worked"
  - "probe on 2026-10-26: a new ad hoc login item in ~/Library/LaunchAgents started v2 after SIGTERM only when the 60 s ThrottleInterval ended, and started v3 0.7 s after a bootout and a bootstrap, with no new BTM notice; it did not show the constraint error"
---

# 0126. The HUD restart reloads the login item

## Context and Problem Statement

ADR 0125 restarts the HUD with SIGTERM and lets KeepAlive start the new app. On this Mac, the
first start of each new brew build still fails with `Launch Constraint Violation`. The HUD is ad
hoc signed, and Background Task Management keeps a launch constraint for its login item that
names the cdhash (the hash of one binary). A new build has a new cdhash, so the start fails until
launchd updates that constraint, 60 s later. A login item that launchd loads fresh gets a first-run
update and starts at once.

SIGTERM also waits out the 60 s `ThrottleInterval` when the HUD ran for less than 60 s.

## Considered Options

- Boot the login item out, wait until it is gone, and bootstrap its plist again (chosen).
- Sign the HUD with a Developer ID, so the constraint names a team and not one binary.
- Keep SIGTERM and the 60 s retry of ADR 0125.

## Decision Outcome

Chosen: the first option, because it needs no certificate and reuses the reload that
`swarm setup hud` already does.

- `swarm hud restart` reloads the HUD login item when its plist exists and does nothing when it
  does not. It loads the plist that is there and does not write a new one.
- The page's Restart, `scripts/release.sh`, and `scripts/install-local.sh` call it. This replaces
  the SIGTERM and the `kickstart` of ADR 0125. Its 502 and 500 rule for the page stays.

### Consequences

- Good: no throttle wait, and a new build starts in about 1 s if a fresh load clears the old
  constraint.
- Bad: no probe showed the constraint error, so it is not proved that a reload clears it. The next
  release is the test; if the HUD still starts 60 s late, this record is wrong.
- Bad: on a Mac that has not used up its BTM notices, a reload might show a "can run in the
  background" notice. The probe showed none for a reload of a known item.
