---
name: checked-units
description: "Split multi-step work into small units that each end in a passing check before the next unit starts. Use when you plan a sweep, a migration, a run of similar edits, or a stack of commits."
license: MIT
metadata:
  kind: rule
  owns: "the order of multi-step work as small checked units"
  serves: "build"
  needs: "a task with more than one step"
  gives: "an ordered list of units, each with its check"
  conflicts: ""
  users: "pair, deliver, flow"
  source: "skills/sequence-verifiable-units/SKILL.md (whole text, moved here 2026-10-13); mined row s4#15 (one record first)"
---

## Rules
- Order the work as small units. Each unit ends in a check, and the next unit starts only after that check passes. A break found at its own unit is cheap to find; a break found after a batch is buried under the work built on it.
- A unit is the smallest change that leaves a checkable state. A helper lands in the same unit as its first caller, so the check passes at every commit. A failing test lands before the fix that makes it pass. A removal lands before the reshape that needs it.
- Each unit is one bracket: a known-good state, one change, the check, then the next unit. Start from a clean base, so every check measures against the real baseline. Never save the checks for one final batch.
- Order the units so the stack proves itself. A reviewer reads it as an argument, red then green, scaffold then feature, and each commit stands on its own.
- Two seats may build fix rows with declared, separate files at the same time, under the `deliver` skill's Parallel fix seats (opt-in) section. Each fix still ends in its own check at its place in the stack.
- When a data change can touch many records and the task allows a sample, the first unit runs it on one record and checks that record before the rest run.
- The check inside each unit follows the kit block `blocks/prove-it-works/SKILL.md`.

## Example
Initial state: a migration renames a field across twelve files.
Action: one unit is one file plus its test. The loop is edit, run the test, commit, twelve times.
Result: when file 7 breaks, its own test fails at unit 7. A run that edits all twelve files and tests once at the end finds the broken file by search, not by position.

## Check (script)
- none: whether each commit passed its check is in the task record, not in the commit range alone.

## Check (judge)
- Does each unit in the task record name its check and that check's result before the next unit starts? Quote two units in a row.
- Does any helper, field, or export land in a commit before its first caller? Quote the commit, or write "none".
- Does a failing test land before the fix that makes it pass? Quote both commits, or write "no test-first fix".
