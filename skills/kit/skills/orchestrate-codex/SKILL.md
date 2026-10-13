---
name: orchestrate-codex
description: Bind provider-neutral multi-agent workflow requirements to Codex orchestration mechanics. Use when a Codex root session runs a portable skill that requests isolated workers, routed models, durable results, or parallel fan-out.
---

# Orchestrate from Codex

Run worker seats as the kit block `blocks/run-workers/SKILL.md` says, and write every brief as the
kit block `blocks/worker-brief/SKILL.md` says. The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder. Read both blocks in full. This file adds only what is different on this runtime.

## On this runtime

- Load the host skill that the `[agent-host: ...]` contract names.
- This section binds an orchestrator session. Codex collaboration workers (`spawn_agent`) run
  in-process as background workers. Use them unless the host contract or the workflow requires a
  visible pane.
- A worker session follows its host worker contract and the worker rules of
  `blocks/worker-brief/SKILL.md` instead.
