---
status: accepted
date: 2026-11-26
deciders: [user]
related: ["0136"]
informed-by:
  - 'owner, 2026-11-26: accepted "at most 160 characters, with the Use when part inside the first 120, enforced by a kit check script" with "Yes".'
  - 'report ~/.swarm/reports/2026-11-25-skill-model-fit.md: Codex shortens long skill descriptions from the end (OpenAI, 11 Sep 2026), so a trigger at the end is the part it cuts.'
---

# 0172. Kit descriptions fit 160 characters with the trigger first

## Context and Problem Statement

ADR 0136 checks each skill description with a 40-word "Use when" rule. Codex shortens long
descriptions to fit its skill list, and it cuts from the end. Most kit descriptions put "Use when"
last, so a cut drops the trigger, and the skill no longer loads when it should.

## Decision Outcome

Every `SKILL.md` and `PRESET.md` description in the kit has at most 160 characters, and its
"Use when" part starts within the first 120. The text counts without its YAML quotes.

## Confirmation

`skills/kit/scripts/test_plugin_skills.py` (`test_each_description_is_short_and_says_when`) owns
the limit and the "Use when" check; `test_kit_block_frontmatter` no longer checks the description.

Supersedes ADR 0136 only where its Confirmation names the 40-word check.
