---
name: lean-review
description: "List only code to cut in a diff or repo, one tagged line per finding, plus the ledger of marked shortcuts. Use when asked what to delete, for an over-engineering review, or a repo audit."
license: MIT
metadata:
  kind: step
  owns: "the cut list for a diff or a repo, and the shortcut ledger"
  serves: "review"
  needs: "a diff or a repo"
  gives: "ranked cut lines with their replacements, a net line, and the shortcut ledger"
  conflicts: ""
  users: "review, planner"
  source: "ponytail pack: skills/ponytail-review, ponytail-audit, and ponytail-debt SKILL.md (MIT, Copyright (c) 2026 DietrichGebert); council 2026-10-30-skill-consolidation"
---

## Rules
- Scope is extra complexity only. Bugs, security, and speed go to the normal review rows. This step lists cuts and applies none.
- Judge each piece by `blocks/lean-ladder/SKILL.md` and `blocks/when-to-extract/SKILL.md`. Tag each finding with one word:
  - `delete:` dead code, unused options, or a feature that nothing calls. Nothing replaces it.
  - `stdlib:` hand-written code for what the standard library does. Name the function.
  - `native:` a dependency or code for what the platform does. Name the feature.
  - `yagni:` an interface with one implementation, config that nobody sets, or a layer with one caller.
  - `shrink:` the same logic in fewer lines. Show the shorter form.
- Write one line per finding: `<file>:L<line>: <tag> <what>. <replacement>.`
- For a diff, keep the diff order. For a whole repo, rank the findings by the biggest cut first.
- End with `net: -<N> lines possible` (and `-<M> deps` for a repo). With nothing to cut, write `Lean already.`
- A single smoke test or a small `assert` self-check is the minimum, not extra code. Never list it.
- For the shortcut ledger, run `rg -n '(#|//) ?(lean|ponytail):'` and write one row per marker: `<file>:<line>, <what>. limit: <limit>. upgrade: <trigger>.` Tag a marker with no upgrade trigger `no-trigger`. End with `<N> markers, <M> with no trigger.`

## Example
Initial state: a diff adds a 27-line `EmailValidator` class and imports `moment` for one date format.
Action: write `validator.ts:L12: stdlib: 27-line validator class. "@" check, 1 line; the confirmation mail is the real check.` and `dates.ts:L4: native: moment for one format call. Intl.DateTimeFormat, 0 deps.`
Result: two findings and `net: -26 lines possible, -1 dep`.

## Check (script)
- none: the findings live in the review record, which differs for each run.

## Check (judge)
- Does each finding have a file, a line, one tag, and a named replacement? Quote two findings.
- Does any finding target a bug, security, speed, or a smoke test? Quote it, or write "none".
