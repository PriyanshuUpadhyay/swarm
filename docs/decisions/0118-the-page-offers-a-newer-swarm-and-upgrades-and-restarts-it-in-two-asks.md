---
status: accepted
date: 2026-10-23
deciders: [user]
related: ["0079", "0087", "0095"]
informed-by:
  - 'owner, 2026-10-23: "Whenever there is a new update available, it should give that notification to the user and also let the user upgrade it from the UA itself and start the swarm ui server as two step ask"'
  - 'owner, 2026-10-23: "id hud is setup in the system then it should restart if it has new changes. write the adr."'
  - "scripts/release.sh: after a release it runs brew update, brew upgrade swarm, and kickstarts the HUD login item"
  - "tmp/flow/2026-10-23-swarm-updated-notification-somewhere/03-contracts.md (routes and the HUD change test)"
---

# 0118. The page offers a newer swarm, and upgrades and restarts it in two asks

## Context and Problem Statement

swarm ships as one Homebrew formula (ADR 0079), and the owner keeps the web page or Swarm.app
open for days. Nothing told the owner that a new version was out. An upgrade took a terminal:
`brew update`, `brew upgrade swarm`, `swarm ui` to replace the server of the old build, and a
`launchctl kickstart` for the usage HUD (ADR 0087).

## Considered Options

- The page asks the server about the tap, then upgrades on one press and restarts on a second
  press (chosen).
- One press that upgrades and restarts together.
- Ask the newest GitHub release instead of the tap formula.
- A `swarm upgrade` CLI verb only, with no page notice.

## Decision Outcome

Chosen: the first option, because the owner asked for two asks. The upgrade can take minutes
and the restart breaks the open page for a moment, so the owner picks when each happens.

- `GET /api/update` reads `Formula/swarm.rb` of `PriyanshuUpadhyay/homebrew-tap` with
  `/usr/bin/curl` and takes the version from its release URL. It is the version that
  `brew upgrade` installs; the newest release can be ahead of the tap. The server keeps the read
  for 1 h. The page asks on load and every 6 h.
- Only a swarm that runs from `<prefix>/Cellar/swarm/` gets the notice. A cargo or checkout
  build gets `latest: null`, and its upgrade and restart calls fail with 409.
- The first ask, Upgrade, runs `brew update --quiet` and `brew upgrade swarm` in a task that a
  closed page does not stop, and checks that `<prefix>/opt/swarm/bin/swarm --version` is newer.
  A `.local` build that brew keeps is an error that names `brew reinstall`.
- When the HUD login item exists, the upgrade hashes the HUD executable before and after. A new
  hash marks the HUD as changed.
- The second ask, Restart server, kickstarts the HUD login item with `-k` when it changed, then
  starts the new `swarm ui --port <this port>`. That `swarm ui` stops the server of the old build
  and starts its own, and the page reloads on the new build's `hello` frame.
- One toast carries both asks. "Not now" and "Later" close it until the next page load.

### Consequences

- Good: the owner sees a new version and installs it from the page, with no terminal.
- Good: the HUD restarts only when its app changed, and only when the owner set it up.
- Bad: the server now reads raw.githubusercontent.com, at most once an hour. The request sends
  no data beyond the HTTP request itself.
- Bad: a HUD that runs without the login item, such as one that `swarm ui` started, keeps the old
  build until it restarts.
- Bad: a server started with `swarm serve` on a random port cannot be restarted this way,
  because the new `swarm ui` finds no record of it.
