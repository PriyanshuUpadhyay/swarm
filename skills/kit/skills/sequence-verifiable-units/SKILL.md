---
name: sequence-verifiable-units
description: Owner rule for how multi-step work is ordered. Apply to a sweep, a migration, a run of similar edits, and to how commits and chunks stack. Each unit ends in a checkable state, the check runs before the next unit, and the order lets a reviewer replay the work.
disable-model-invocation: true
---

# Sequence work into verifiable units

Order work as small units that each end in a check, and do not advance until the current one
passes. Fix rows with declared disjoint files may be built by two seats at once under `deliver`'s
Parallel fix seats (opt-in) section; each still ends in its own check at its stack position.
A break caught at its own unit is cheap to find. A break caught after a batch is buried
under work built on it.

A unit is the smallest change that leaves a checkable state. A helper lands in the same unit as
its first caller, so the check passes at every commit. A failing test lands before the fix that
makes it pass. A subtraction lands before the reshape that needs it.

Each unit is one bracket: a known-good state, one change, the check, then the next unit. Start
from a clean base, so every check measures against the real baseline. Never defer the checks to
one final batch.

Order the units so the sequence proves itself. A reviewer reads the stack as an argument, red then
green, scaffold then feature, and each commit stands on its own.

Example. A migration renames a field across twelve files. The unit is one file plus its test, and
the loop is edit, run the test, commit, twelve times. A run that edits all twelve and tests once
at the end finds the broken file by search instead of by position.

The check inside each unit follows `~/.claude/skills/prove-it-works/SKILL.md`.
