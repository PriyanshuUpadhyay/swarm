---
name: base-rules
description: "Keep the rules every step keeps that no instruction file owns: facts only from the source of truth, and real over drawn. Use when any workflow step runs."
license: MIT
metadata:
  kind: rule
  owns: "the rules every step of every workflow keeps"
  serves: "every step"
  needs: "source_of_truth"
  gives: "steps that keep the base rules"
  conflicts: ""
  users: "implement"
  source: "html-plan pack: skills/html-plan/SKILL.md:82 (real over drawn); docs/council 2026-10-13 block rules (N4: no copy of the rules that agent instructions own)"
---

## Rules
- Facts come only from {{source_of_truth}}. If it does not say it, do not write it.
- Real over drawn. Cite the file and line for a fact or a code claim.
- The session's agent instructions (`AGENTS.md`, `CLAUDE.md`) own scope, evidence for done, and how to explain. This block does not restate them.
- If two rules disagree, say which two and which one you followed.

## Example
Initial state: a step wants to write "the agent retries three times". Action: find the line in {{source_of_truth}} that says so and cite it. Result: the claim carries its `path:line`, or it is cut.

## Check (script)
- none: each rule needs meaning, so its checks are judge checks.

## Check (judge)
- Does every fact in the result trace to a line in {{source_of_truth}}? Quote each doubtful fact and its source line, or write "no source line".
- Does each claim of done name its evidence? Quote the claim and its evidence.
