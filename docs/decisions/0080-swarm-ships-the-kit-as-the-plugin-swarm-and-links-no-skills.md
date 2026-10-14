---
status: accepted
date: 2026-10-14
deciders: [user]
supersedes: ["0062", "0066"]
amends: ["0079"]
related: ["0076"]
informed-by:
  - 'owner, 2026-10-14: "We don''t care about the skills here. Let them be there. Swarm will only handle the plugin that we are building around blocks. It will not provide any skills to be added."'
  - "Release 0.13.1 shipped libexec/skills/kit without blocks/, .claude-plugin/ or plugin.json, so each skill that reads blocks/<id>/SKILL.md found nothing"
  - "swarm setup --plan --only skills on 2026-10-14 planned a link for each kit skill in ~/.claude/skills, ~/.agents/skills and ~/.gemini/config/skills"
---

# 0080. swarm ships the kit as the plugin swarm, and links no skills

## Context and Problem Statement

ADR 0062 bundled the kit and had Setup link each kit skill into the three CLI skill folders. ADR
0066 let the owner edit those bundled skills from the app. Since then, the kit skills are made from
blocks: a skill reads `blocks/<id>/SKILL.md` two folders up from its own folder, so a link to one
skill folder cannot reach them. The kit is now the plugin `swarm`, with `.claude-plugin/` and
`plugin.json` at its root. The owner keeps the skills that are already in the CLI folders as they
are.

## Considered Options

- Keep 0062: Setup links each kit skill, and the plugin is extra.
- swarm ships the kit as the plugin `swarm` and links no skills.
- swarm ships neither, and the kit is a separate download.

## Decision Outcome

Chosen: swarm ships the kit only as the plugin `swarm`, because the blocks then reach every skill,
and swarm does not write into skill folders that the owner manages.

- The release tarball carries `skills/kit/` whole in `libexec/skills/kit` (amends 0079), without
  `.git*`, `__pycache__` and `node_modules`. `ui/Tools/build.sh` runs `test_block_refs.py` on the
  packed copy, so a release with a missing block fails to build.
- swarm adds no skill to `~/.claude/skills`, `~/.agents/skills` or `~/.gemini/config/skills`, and
  it does not change or remove the skills that are there.
- Superseded: 0062 (skill links in Setup) and 0066 (edits of bundled skills from the app; 0076
  already took the Skills editor out of the web UI).

## Consequences

- Good: one copy of each block serves every skill, and the formula and the checkout hold the same
  plugin.
- Good: the owner's skill folders are the owner's, so no managed-edit record, consent step or undo
  for skill links is needed.
- Bad: Setup's `skills` group, `swarm skills refresh` and the link catalog in `src/skills.rs` still
  plan links until a follow-up change removes them.
- Bad: `swarm-orchestrator` and `swarm-voice` still ship beside the kit in `libexec/skills`, and
  their place under this rule is not decided.
- Bad: each CLI must install the plugin by its own command, and swarm does not do it for them yet.

## Links

- Supersedes: 0062, 0066. Amends: 0079. Related: 0076.
