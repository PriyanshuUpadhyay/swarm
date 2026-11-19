---
status: accepted
date: 2026-11-19
deciders: [user]
amends: ["0145"]
related: ["0146", "0149"]
informed-by:
  - 'owner, 2026-11-19: "Swarm task land should not merge by default but rather open a pr on top of default branch"'
  - 'owner answer A, 2026-11-19: PR by default, direct merge behind --merge, --pr still accepted'
---

# 0150. `swarm task land` opens a PR by default

## Context and Problem Statement

ADR 0145 made `swarm task land` merge straight into the default branch and push it.
A chat could then change the default branch with no review. The PR path needed an extra `--pr` flag.

## Decision Outcome

`swarm task land [<name>]` pushes the task branch and opens a PR into the default branch.
`swarm task land [<name>] --merge` does the direct merge that was the default before.
`--pr` is still accepted and means the same as no flag. `--pr` with `--merge` is a usage error.
The landing request file format does not change.

## Consequences

- Good: no chat changes the default branch until a person merges the PR.
- Bad: a hub with no origin cannot open a PR, so it needs `swarm task land --merge`.
- Neutral: the task worktree stays until the PR merges. Then `swarm task sweep --apply` removes it (ADR 0144).
