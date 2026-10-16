# Agents

This repo ships two skills for agent CLIs that take part in a swarm.

- If your pane was started by `swarm launch` or `swarm spawn`, read
  `skills/kit/skills/swarm-voice/SKILL.md` and follow it.
- If you are asked to run a swarm session, read `skills/kit/skills/swarm-orchestrator/SKILL.md`.

Inside this repo, Claude Code finds them through `.claude/skills` and AGY through `.agents/skills`; Codex reads this file, so open the skill you need before you act. Both ship in the plugin `swarm`, so `swarm setup plugin` installs them in every agent CLI on this machine.
