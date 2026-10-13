---
name: orchestrate-claude
description: Bind provider-neutral multi-agent workflow requirements to Claude's orchestration mechanics. Use when a Claude root session runs a portable skill that requests isolated workers, routed models, durable results, or parallel fan-out.
---

# Orchestrate from Claude

Run worker seats as the kit block `blocks/run-workers/SKILL.md` says, and write every brief as the
kit block `blocks/worker-brief/SKILL.md` says. The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder. Read both blocks in full. This file adds only what is different on this runtime.

## On this runtime

- For Herdr, the host reference is `references/host-swarm.md` in this skill's folder.
- Result acceptance is the swarm inbox message.
- A worker session follows the worker rules of `blocks/worker-brief/SKILL.md`.
- Native background subagents are allowed unless the host contract or the workflow requires a
  visible pane. Every Workflow `agent()` call pins `model: "sonnet"` or `model: "haiku"`, never
  Opus or Fable, because a workflow fans out to many agents.
