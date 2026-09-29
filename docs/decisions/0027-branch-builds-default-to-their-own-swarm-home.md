---
status: accepted
date: 2026-09-29
deciders: [user]
related: ["0015", "0016"]
informed-by:
  - "User answer on 2026-09-29: branch builds are to be kept apart"
  - "2026-09-29: a ui-polish build installed as the global swarm migrated the real ~/.swarm to schema 3"
---

# 0027. Branch builds default to their own swarm home

In the context of development builds of the swarm CLI and the Swarm app, facing ADR 0015's rule that
a per-branch `SWARM_HOME` is set by hand, which a branch build installed as the global `swarm` went
around and so migrated the owner's real `~/.swarm`, we chose that a build records its git branch and,
when `SWARM_HOME` is unset, a build from any branch other than `main` uses `~/.swarm-<branch>` while a
`main` build or a build with no branch keeps `~/.swarm`, an explicit `SWARM_HOME` always wins, and the
app passes its home to every `swarm` process it starts, and neglected keeping the hand-set convention
and bundling a CLI inside each app, to keep branch data apart by default, accepting that a branch
build starts with an empty home and that a branch name must map to a safe folder name.
