---
status: accepted
date: 2026-11-18
deciders: [user]
amends: ["0027"]
related: ["0036", "0048"]
informed-by:
  - 'owner, 2026-11-18: "yeah need a gurad… add the guard in current branch"'
  - '2026-11-18: a review seat ran its scratch build of a branch in its pane, which had SWARM_HOME set to the owner home, and migrated the real ~/.swarm to the branch schema'
---

# 0148. A dev build refuses the owner's home

## Context and Problem Statement

ADR 0027 keeps a dev build in `~/.swarm-<branch>`, but an explicit `SWARM_HOME` always wins. The
app gives every pane `SWARM_HOME` set to the owner's home, so a branch build that an agent runs in
a pane opened the real `~/.swarm`. It migrated the store to the branch's schema, and the installed
release then refused the store.

## Decision Outcome

**A dev build refuses a `SWARM_HOME` that names the owner's home.** The owner's home is the home of
the account that runs the build, read from the user database (`getpwuid_r`) and not from `HOME`.
So a test that sets both `HOME` and `SWARM_HOME` to a scratch folder still runs. `paths::home`
compares the two folders after it resolves them, so a trailing `/` or a symlink does not get past
it. The error names
the branch and the folder that the build uses when `SWARM_HOME` is unset. A release build
(`SWARM_RELEASE_BUILD=1`) is not changed, and a dev build still takes any other explicit
`SWARM_HOME`, such as a test's temporary folder.

There is no override. A dev build that must read the real store is a release build made with
`scripts/install-local.sh`.

### Consequences

- Good: no dev build, run by hand or by an agent, can open, migrate, or write the real store in
  `~/.swarm`. A dev build still takes the shared `~/.swarm/trust.lock` (ADR 0043), which holds no data.
- Bad: when the user database gives no home, the guard cannot check, so the build takes `SWARM_HOME`.
- Bad: a dev build in an app pane must run with `SWARM_HOME` unset or set to another folder.
