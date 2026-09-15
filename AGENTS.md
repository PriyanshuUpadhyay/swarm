# Agents

This repo ships two skills for agent CLIs that take part in a swarm.

- If your pane was started by `swarm spawn` (`SWARM_AGENT_ID` is set in your environment), read
  `.claude/skills/swarm-voice/SKILL.md` and follow it.
- If you are asked to run a swarm session, read `.claude/skills/swarm-orchestrator/SKILL.md`.

Claude Code loads these skills from `.claude/skills` on its own. Codex and AGY read this file, so
open the skill you need before you act.
