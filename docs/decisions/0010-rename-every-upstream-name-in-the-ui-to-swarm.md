---
status: accepted
date: 2026-09-17
deciders: [user]
supersedes: 0009
related: [0001, 0002]
informed-by: []
---

# 0010. Rename every upstream name in the UI to Swarm

In the context of shipping the forked UI as our own Swarm app, facing the owner's request that no
reference to Bloom remain, we chose to rename every target, module, type, file, env var, on-disk
path and piece of prose in `ui/` and `docs/` to Swarm (`SwarmCore`, `swarm-bridge`,
`swarm-sleep-helper`, `SWARM_UI_*` env vars, `swarm.sqlite`, `~/swarm/workspaces.noindex`,
`.swarm/settings.toml`), guarded by a house rule that fails on the old name, and to keep 0009's
removal of the install ping, crash reports, feedback upload, updater and Spatie branding, and
neglected keeping the upstream code names as 0009 chose, so the app and its code read as ours,
accepting that later changes from Spatie's upstream must be ported by hand, that env vars take
the `SWARM_UI_` prefix because the swarm CLI owns plain `SWARM_*` names, and that decision records,
git history, `ui/LICENSE.md` and the `ui-bloom` branch name keep the old name.
