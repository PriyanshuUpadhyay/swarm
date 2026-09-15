# Agents

This repo ships two skills for agent CLIs that take part in a swarm.

- If your pane was started by `swarm spawn` (`SWARM_AGENT_ID` is set in your environment), read
  `skills/swarm-voice/SKILL.md` and follow it.
- If you are asked to run a swarm session, read `skills/swarm-orchestrator/SKILL.md`.

Claude Code loads them through `.claude/skills`, which links to `skills/`. Run `sh scripts/install-skills.sh` once to link them into every agent CLI on this machine. Codex and AGY read this file, so
open the skill you need before you act.
