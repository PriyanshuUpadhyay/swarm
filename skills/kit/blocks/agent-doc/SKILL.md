---
name: agent-doc
description: "Write text that an agent reads, such as a skill, block, or AGENTS.md: trigger words first, detail only where needed, checkable step ends, targets over bans. Use when you write or change agent instructions."
license: MIT
metadata:
  kind: rule
  owns: "how agent instructions are worded and split"
  serves: "build"
  needs: "a skill, block, or instruction file to write or change"
  gives: "instructions with clear triggers, checkable steps, and one owner per meaning"
  conflicts: ""
  users: "implement, planner"
  source: "mattpocock-skills: skills/productivity/writing-for-agents/SKILL.md (MIT); anthropic-skills and Codex: skill-creator/SKILL.md; council 2026-10-30-skill-consolidation"
---

## Rules
- The block format is owned by `blocks/blocks.toml`. This block owns the wording.
- A description or pointer line decides when the agent reads the material. Put its key word first, give one trigger per real case, and cut words that the body already says.
- Keep in the file what every case needs. Move to a separate file what only some cases need, behind a pointer that names when to read it.
- Keep a concept's definition, rules, and limits under one heading.
- End each step on a condition that the agent can check, such as "every changed model is listed", not "understand the code".
- State the target behavior. Use a ban only for a hard limit that has no positive form, and give the target next to it.
- Keep each meaning in one place. Do not copy what a command, a config file, or `--help` already shows; write down only what the agent cannot find by looking.
- Delete each sentence that does not change what the agent does by default.
- Prove a change to the text by the trial rule in `blocks/prove-it-works/SKILL.md`.

## Example
Initial state: a skill description reads "Helps with many tasks related to databases, migrations, schemas, SQL, and data."
Action: rewrite it as "Migrate a database schema safely. Use when a change adds, renames, or drops a column." Move the rollback detail into `references/rollback.md`, with the pointer "read it when a migration must be undone".
Result: the description has one trigger per case, and the rollback text loads only on that branch.

## Check (script)
- none: whether a trigger or a condition is clear is a question of meaning.

## Check (judge)
- For each changed description or pointer, quote it and say which word triggers it and for which case.
- Does each changed step end on a condition the agent can check? Quote the step and its condition.
- Does each new ban come with its target behavior? Quote both, or write "no ban".
