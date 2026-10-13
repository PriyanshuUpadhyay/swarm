---
name: orchestrate-agy
description: Bind provider-neutral multi-agent workflow requirements to AGY orchestration mechanics. Use when an AGY root session runs a portable skill that requests isolated workers, routed models, durable results, or parallel fan-out.
---

# Orchestrate from AGY

Run worker seats as the kit block `blocks/run-workers/SKILL.md` says, and write every brief as the
kit block `blocks/worker-brief/SKILL.md` says. The kit's blocks are in `~/.claude/blocks/`, a link to the kit's `blocks/` folder. Read both blocks in full. This file adds only what is different on this runtime.

## On this runtime

- Load the host skill that the `[agent-host: ...]` contract names.
- A worker session follows the worker rules of `blocks/worker-brief/SKILL.md`.
- Native AGY subagents are allowed unless the host contract or the workflow requires a visible
  pane.
- Never pass a prompt as a launch argument. Send the first task only after the host has detected
  an input-ready AGY session.
