---
status: accepted
date: 2026-10-12
deciders: [owner]
related: ["0012", "0036"]
informed-by:
  - "owner, 2026-10-12: implement the adapter in the repo itself, not in the home"
  - "tmp/flow/2026-10-12-superset-adapter/03-contracts.md"
  - "Superset source main 5021353, 2026-10-09: host-service terminal/env.ts sets SUPERSET_TERMINAL_ID; terminals send wraps text in bracketed paste and strips control bytes"
---

# 0067. Superset is a host, detected from its terminal id

In the context of swarm's host adapters, facing a chair that runs in a Superset terminal with no
host variable of its own, we chose to detect Superset from `SUPERSET_TERMINAL_ID` after Herdr and
before tmux, and to ship `adapters/superset.conf` without `key`, `interrupt`, and `attach`, and
neglected a disk-only adapter plus `SWARM_ADAPTER=superset` in every agent env, to achieve a chair
that needs no setup variable, accepting that `swarm answer` and `swarm interrupt` do not work on
this host and that a tmux-on-Superset user must set `SWARM_ADAPTER=tmux`.
