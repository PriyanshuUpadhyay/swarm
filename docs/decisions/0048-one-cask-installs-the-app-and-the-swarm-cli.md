---
status: accepted
date: 2026-10-06
deciders: [user]
related: ["0027", "0034", "0042", "0043"]
informed-by:
  - "Owner answers Q1-Q4 on 2026-10-06, flow tmp/flow/2026-10-06-unify-swarm (03-contracts.md): Q1 cask binary stanza; Q2 pointer formula now, removed next release; Q3 one alert per path-and-version pair; Q4 make install puts no cargo build on PATH, the check runs only in release builds"
  - "Incident: the 0.8.0 app ran /opt/homebrew/bin/swarm 0.7.3, which has no managed verb, and Managed Changes broke (tmp/flow/2026-10-06-unify-swarm/01-frame.md)"
  - "Homebrew source: cask/artifact/symlinked.rb (a formula that owns the path makes the cask skip its link), cmd/upgrade.rb (formulae upgrade before casks), cask/dsl/conflicts_with.rb (only :cask)"
  - "Owner answer on 2026-10-06, review tmp/review-check/range-7122605-3bf9ba4-01 u22 L-12: branch home on main too"
---

# 0048. One cask installs the app and the swarm CLI

## Context and Problem Statement

The tap shipped two packages, a `swarm` formula that built the CLI and a `swarm-app` cask that
depended on it. They upgrade apart, so the app and the `swarm` on PATH can come from two releases.
The 0.8.0 app ran a PATH CLI 0.7.3 that has no `managed` verb, and Managed Changes broke. The app
now runs its bundled helper, but Terminal and agents that swarm did not start still run the PATH
copy against the shared `~/.swarm/swarm.db`.

## Considered Options

- One cask: its `binary` stanza links `swarm` on PATH to `Swarm.app/Contents/Helpers/swarm`, it
  depends on the tmux formula, and the `swarm` formula becomes a pointer for one release.
- The app makes the PATH link at launch as a managed edit (ADR 0042).
- Keep the formula and the cask, and pin the cask to the formula's version.

## Decision Outcome

Chosen: one cask with a `binary` stanza, because brew makes and removes the link itself and swarm
makes no write outside its home. For this release the `swarm` formula is a pointer: it is
`deprecate!`d with `replacement_cask:` and installs no `bin/swarm`, so `brew upgrade` (formulae
first) frees the link before the cask takes it. The next release removes the formula. At launch a
release build of the app compares the PATH `swarm --version` line with its helper's; when they
differ it shows one alert per path-and-version pair with Copy Command and Not Now, and it shows
nothing when a version cannot be read. The app always runs its bundled copy. `make install` puts
no cargo build on PATH. Rejected: the app-made link, because swarm would write into the Homebrew
prefix and a DMG-only install has no prefix to own; two pinned packages, because they still
upgrade one at a time.

### Consequences

- Good: one `brew upgrade` moves the app and the CLI together, and the app names any other copy
  that still wins on PATH.
- Good: tmux comes with the cask, so no formula of this tap is needed.
- Bad: the cask needs macOS 26, so a CLI-only user on an older macOS or on Linux loses the brew
  route and uses `cargo install --path .`.
- Bad: a cask cannot refuse the old formula (`conflicts_with formula:` does not exist), so a
  machine that upgrades the cask alone keeps the old link until the launch alert names it.

## Amendment 2026-10-06: only the release build uses ~/.swarm

Deciders: [user]. Informed by the owner answer "Branch home on main too".

`make install` on `main` or on a detached HEAD ran `swarm init` with ADR 0027's home rule, which
gave those builds `~/.swarm`. A HEAD with a migration that the latest release lacks migrated the
real database, and the cask's `swarm` on PATH then refused it ("database made by another swarm
build"). So this amends ADR 0027's rule for `main` and for no branch. Only the build that
release.yml ships uses `~/.swarm`: it sets `SWARM_RELEASE_BUILD=1`, and `build.rs` then stamps the
branch `""`. Every other build is a dev build. A dev build from `main` uses `~/.swarm-main`, and one
from a detached HEAD, or from a folder that git cannot read, stamps `HEAD` and uses
`~/.swarm-head+4b9253d8ff1ee183`. `HEAD` is not a valid branch name, so that folder meets no
branch. An explicit `SWARM_HOME` still wins. `Tools/build.sh` writes the helper's stamped branch
into Info.plist, so the app and its helper agree. The app's PATH check still runs only when that
branch is `""`.

- Bad: `cargo install --path .` without `SWARM_RELEASE_BUILD=1` is a dev build, so a CLI-only
  user who wants `~/.swarm` sets that variable or `SWARM_HOME=$HOME`.
- Bad: a `main` build starts with an empty `~/.swarm-main`.
