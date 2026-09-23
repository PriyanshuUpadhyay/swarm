---
status: accepted
date: 2026-09-16
deciders: [user]
related: [0001, 0004]
informed-by: []
---

# 0005. swarm owns roles, accounts and usage for the UI

In the context of showing roles, accounts and usage in Bloom, facing two existing owners of those
decisions (`agent-routing.mjs` for roles, `yelo` for Claude and Codex accounts and usage), we chose
to have swarm wrap both behind `swarm roles`, `swarm accounts` and `swarm usage` with JSON output,
with multiple accounts as a general rule for every provider (AGY gets an account source later), and
Bloom calling only swarm, and neglected Bloom calling `agent-routing.mjs` and `yelo` directly, so
the CLI launcher and the UI always agree, accepting that swarm now depends on two external tools
and a change in their JSON must be translated in swarm.
