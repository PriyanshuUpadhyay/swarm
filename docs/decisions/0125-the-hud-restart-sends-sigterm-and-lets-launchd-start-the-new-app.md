---
status: accepted
date: 2026-10-26
deciders: [user]
amends: ["0118"]
related: ["0087"]
informed-by:
  - 'owner, 2026-10-26: "yeah let''s fix th ehud thing." and "do it" for this record'
  - "release 0.0.43: `launchctl kickstart -k` took over 60 s, and launchd logged `OS_REASON_CODESIGNING | Launch Constraint Violation` on the first start of each new brew build"
  - "test on 2026-10-26: a first `kickstart -k` took 0 s and a second one at once took 60 s; `launchctl kill SIGTERM` returned at once and launchd started a new HUD at once"
---

# 0125. The HUD restart sends SIGTERM and lets launchd start the new app

## Context and Problem Statement

ADR 0118 restarts a changed HUD with `launchctl kickstart -k` and gives each launchctl call 10 s.
The HUD's login item sets `ThrottleInterval` to 60 (ADR 0087), and `kickstart -k` waits until
60 s after the last start. So a restart soon after a start ran past the 10 s limit, and the page
said that the HUD did not restart, while the HUD came back later.

Also, in the 0.0.43 release, the first start of each new HUD build that brew installed failed
with `Launch Constraint Violation`, and launchd's next try, 60 s later, worked. A local build of
a small ad hoc app did not fail in the same way, so the cause of that first failure is not known.

## Considered Options

- Send SIGTERM to a running HUD, and let KeepAlive start the app that `opt` names (chosen).
- Keep `kickstart -k` with a limit above 60 s.
- Run `kickstart -k` in the background and do not wait for it.
- Lower `ThrottleInterval`.

## Decision Outcome

Chosen: the first option, because `launchctl kill` returns at once and launchd already owns the
start and the retry.

- The page's Restart, `scripts/release.sh`, and `scripts/install-local.sh` send
  `launchctl kill SIGTERM` to the login item when the HUD runs. KeepAlive starts the new app
  through `opt`. When that first start fails, launchd tries again after `ThrottleInterval`.
- The scripts start a HUD that does not run with `launchctl kickstart`, as before. The page
  leaves that to the new `swarm ui`, as ADR 0118 says.
- The restart answers 502 when the HUD try fails and 500 when the new server cannot start. The
  409s and the guard's refusals come before the HUD try. So only a 500 or a 502 drops the HUD from
  the page's text, and this replaces the "only a 409 keeps the HUD" line of ADR 0118.

### Consequences

- Good: the restart no longer waits on the throttle, and the page no longer reports a HUD failure
  that launchd fixes by itself.
- Bad: a HUD whose first start fails comes back up to 60 s later, and the page does not say so.
- Bad: the HUD's SIGTERM exit must not count as a successful exit, or KeepAlive does not start
  it. AppKit ends on the signal, so it does not count today.
- Later: find why a new brew-installed ad hoc HUD fails its first start.
