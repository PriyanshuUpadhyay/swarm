---
status: accepted
date: 2026-09-16
deciders: [user]
related: [0001]
informed-by: []
---

# 0002. Keep the Bloom UI inside the swarm repo

In the context of the Bloom-based swarm UI, facing where its code lives, we chose a squashed
`git subtree` of spatie/bloom under `ui/` in the swarm repo and neglected a separate GitHub fork and
a local clone outside the repo, to keep the engine and its UI in one repository and one branch
history, accepting that upstream updates need a manual `git subtree pull` with merge conflicts in
the files we change, and that the repo grows by Bloom's tree.
