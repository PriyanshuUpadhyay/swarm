---
status: accepted
date: 2026-09-17
deciders: [user]
related: [0005, 0007]
informed-by: []
---

# 0008. swarm builds the agent command line for a role

In the context of Bloom starting an agent for a role, facing the launch logic that lives in the
private `swarm-spawn-role.py` and `agent-teammate.py` scripts, we chose to add `swarm launch`, which
resolves the role and builds the Claude, Codex or AGY command line with its model, effort, sandbox
and approval flags, and neglected Bloom calling the Python launcher, so the repository works
without those private scripts and swarm keeps owning roles (ADR 0005), accepting more Rust to keep
in step with the router's fields and that trust prompts are answered by the user in the live pane.
