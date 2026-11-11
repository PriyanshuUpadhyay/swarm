---
name: engineering-standards
description: "Supply engineering rules, marked as protocol, project rule, or advice. Use when a change touches an API payload, a data model or query, a retry or deadline, a quota or limit counter, or an authorization or entitlement read."
license: MIT
metadata:
  kind: rule
  owns: "the engineering standards rows and how to apply them"
  serves: "build, review"
  needs: "the requested behavior and the repo's contracts"
  gives: "the rows that apply, each with its check result"
  conflicts: ""
  users: "implement, review, planner, AGENTS.md"
  source: "skills/engineering-standards/SKILL.md (the skill, made a block 2026-10-31; the rows moved to references/standards.md)"
---

## Rules
- The kit root is two folders up from this file.
Select rules from the requested behavior and existing repository contracts, or say no rule covers
it. Do not ask the user to repeat engineering standards or invent product requirements. The
calling workflow keeps its edit authority and approval steps; this block supplies rules.

**How to read a row.**

`Status` — **MUST** is a protocol requirement. **PROJECT** is a rule this repository adopted.
**ADVICE** is vendor or community guidance. MUST beats PROJECT beats ADVICE. A citation alone does
not prove a rule applies; apply a row only inside its `Scope`.

`Check` — a command that is available, or `judgment`. Use the row's expected output and the tool's
exit status together. A missing command or execution error is **incomplete**, never a pass.
`PRAGMA foreign_key_check` can print violations and still exit 0; other tools can print data on a
pass. Neither exit 0 nor nonempty output alone decides the result. Confirm a check against a
known-bad case before trusting it; a green tool result does not prove a `judgment` row.

**Procedure.**

1. Derive the intended behavior, failure cases, and applicable row IDs from the request and
   repository. Keep them in the existing plan or task record, not a second document.
2. Read the sections of `references/standards.md` in this block's folder that contain those rows, and check their scope.
3. Run each `Check` that fits the actual engine. Report what you could not run.
4. When a `Scope` forks, two rows conflict, or the question is contested, stop and explain the
   trade-off as the kit block `blocks/decision-with-default/SKILL.md` says. Do not rule.
5. End with `pass N / fail N / judgment M / not checked K`. A fail quotes the row
   and the output that failed it.

**Refusals.**

- No `APPROVE` or `REQUEST CHANGES` verdict, and no coverage claim. The review check preset owns those.
- No ADR, no decision record.
- No general security review; the global agent instructions own trust boundaries. Rows A2 and R4 stay, because they are protocol and entitlement rules.
- This block supplies no implementation workflow, migrations, or installs; the caller owns them.
- No answer outside these tables. Say "no applicable rule in this block" instead of improvising.
- No rule without a citation, a runnable check, or a named local incident.

## Example
Initial state: a change adds an API field `limit` where unlimited is `-1`.
Action: rows A9 (variants under `oneOf`) and J2 (stored-to-wire mapping) apply. A9 is ADVICE and the repo has no variant shape, so it is an open trade-off for the owner; J2 is PROJECT, and its judgment fails because no mapping is written.
Result: `pass 0 / fail 1 / judgment 1 / not checked 0`, and the fail quotes J2 and the missing mapping.

## Check (script)
- none: which rows apply is a question of scope, and the checks differ for each row.

## Check (judge)
- Does the step list each applying row ID with its result, and end with the `pass / fail / judgment / not checked` line? Quote the line.
- Does each fail quote the row and the output that failed it? Quote one, or write "no fail".
