# Agents

This repo ships two skills for agent CLIs that take part in a swarm.

- If your pane was started by `swarm spawn` (`SWARM_AGENT_ID` is set in your environment), read
  `skills/swarm-voice/SKILL.md` and follow it.
- If you are asked to run a swarm session, read `skills/swarm-orchestrator/SKILL.md`.

Inside this repo, Claude Code finds them through `.claude/skills` and AGY through `.agents/skills`, both links to `skills/`; Codex reads this file, so open the skill you need before you act. Run `sh scripts/install.sh` once to link them into every agent CLI on this machine (`~/.agents/skills` for Codex, `~/.claude/skills`, `~/.gemini/config/skills` for AGY).
