---
name: review
description: Review in one of three modes, each with its own preset. Check a commit range with a script verdict, walk a GitHub PR into a pending review, or run a cleanup pass. Use when the user asks to review, check a range, review a PR, or clean up a diff.
---

# Review

Example. The owner types `/review` on a branch with open work. The reply asks one question: check
the range, walk a GitHub PR, or run a cleanup pass. The owner answers "walk 42", so the front door
plans from `Preset: review walk`, and the walk starts after the owner approves the plan.

The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.

## Modes

Ask the owner which mode to run, unless the request names it. The answer picks the preset.

| Mode | Runs | Preset and rules |
|---|---|---|
| `check` | a script verdict on a commit range or on local changes | [references/check.md](references/check.md) |
| `walk` | a GitHub PR walk with comments in the reviewer's voice in a pending review | [references/walk.md](references/walk.md) |
| `cleanup` | the six-step cleanup pass on work already written | [references/cleanup.md](references/cleanup.md); its Activation section owns when it runs |

Run the front door in `skills/plan/SKILL.md` with `Preset: review <mode>`, then follow the mode
file. A caller that needs only a verdict, such as `implement` step 06 or the `change-review`
block, runs the check mode's script as `references/check.md` says, with no plan of its own.
