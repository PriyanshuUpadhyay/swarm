# Agents

This repo ships two skills for agent CLIs that take part in a swarm.

- If your pane was started by `swarm launch` or `swarm spawn`, read `skills/swarm-voice/SKILL.md`
  and follow it.
- If you are asked to run a swarm session, read `skills/swarm-orchestrator/SKILL.md`.

Inside this repo, Claude Code finds them through `.claude/skills` and AGY through `.agents/skills`, both links to `skills/`; Codex reads this file, so open the skill you need before you act. Run `swarm skills refresh`, then `swarm setup --plan --only skills`. Review and apply the displayed digest to record links in every agent CLI on this machine (`~/.agents/skills` for Codex, `~/.claude/skills`, `~/.gemini/config/skills` for AGY).
