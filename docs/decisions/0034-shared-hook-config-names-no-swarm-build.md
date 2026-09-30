---
status: accepted
date: 2026-10-01
deciders: [user]
related: ["0021", "0027", "0029"]
informed-by:
  - "Seen on 2026-09-30: after `swarm hooks setup` from the brew CLI, the app helper's `hooks status --json` was {\"codex\":false,\"agy\":false}"
  - "User answer on 2026-09-30: fix the hooks at the root and test each case"
---

# 0034. Shared hook config names no swarm build

In the context of Codex and AGY hooks, whose config every swarm build on a Mac shares (Codex
trusts one hash per event of swarm's session-flag hook, and AGY's `hooks.json` is global), facing
a hook command that held the absolute path of the build that last ran `swarm hooks setup`, so the
brew CLI, the app helper, and a branch build took the slot from each other, we chose a command
text that names no build and runs the pane's own `runs/<session>/bin/swarm` link through
`$SWARM_HOME` and `$SWARM_SESSION_ID`, and prints `{}` where that link is missing, and neglected
`swarm` on PATH and a per-build slot, to let every build see its own agents' hooks after one
setup, accepting that each Mac runs `swarm hooks setup` once more after this change.

## Context and Problem Statement

ADR 0029 has `swarm hooks setup` write swarm's Codex trust entries and AGY group. Both held
`'<current_exe>' hook <provider>`. Codex keys the trust entry by the hook's place in the session
flags, not by the command, so two builds cannot both be trusted. AGY's group is named `swarm`, so
two builds cannot both be in it. A branch build (ADR 0027) in the AGY slot also wrote state for
another build's agents into its own home.

## Considered Options

- The pane's link through env: `"$SWARM_HOME/.swarm/runs/$SWARM_SESSION_ID/bin/swarm" hook ...`.
- `swarm hook ...` found on PATH.
- One Codex group and one AGY group for each build.

## Decision Outcome

Chosen: the pane's link through env.

- `swarm launch` already sets `SWARM_HOME` and `SWARM_SESSION_ID` in each pane and links
  `runs/<session>/bin/swarm` to the launching build, so the hook runs that build on its own home.
- The text is the same for every build, so setup from one build leaves the others' status true.
- Claude's hooks go per process in `--settings` and keep the absolute path.

### Consequences

- Good: the brew CLI, the app, and branch builds share one setup and never ask again for it.
- Good: an AGY session that swarm did not start gets `{}` with no `swarm` on the Mac.
- Bad: the old hash and group no longer match, so `hooks status` is false once after the update.
- Amends ADR 0029, whose consequence "the Codex trust hash follows ... the swarm binary path" no
  longer holds; it still follows Codex's hash rule.

## Pros and Cons of the Options

### `swarm` on PATH
- Good, because the command is shortest.
- Bad, because a login shell can put another build, such as `/opt/homebrew/bin`, first.

### One group for each build
- Good, because each build keeps its exact path.
- Bad, because Codex trusts one entry per place, and each build adds one more AGY group that runs
  on every AGY event.
