---
name: sequence-verifiable-units
description: Owner rule for how multi-step work is ordered. Apply to a sweep, a migration, a run of similar edits, and to how commits and chunks stack. Each unit ends in a checkable state, the check runs before the next unit, and the order lets a reviewer replay the work.
disable-model-invocation: true
---

# Sequence work into verifiable units

The rule is the kit block `blocks/checked-units/SKILL.md`. The kit's blocks are in `~/.claude/blocks/`, a link to the kit's `blocks/` folder. Read the block in full and apply it.
