---
name: review
description: Check a range with a script verdict, walk a GitHub PR, or run a cleanup pass. Use when asked to review, check a range, review a PR, or clean up, on "review continue", or when a big diff reaches delivery.
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
file. `review continue <run>` makes no new plan. It goes to the mode that owns that run folder
(`tmp/review-walk/` is walk, `tmp/review-check/` is check). Walk runs `status` on the run and takes
the first ready step, as `references/walk.md` says. Check runs `verdict <run>` again on the same run,
as the Asks section of `references/check.md` says. A caller that needs only a verdict, such as `implement` step 06 or the `change-review`
block, runs the check mode's script as `references/check.md` says, with no plan of its own.
