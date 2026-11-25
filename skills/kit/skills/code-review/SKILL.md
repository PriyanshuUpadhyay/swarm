---
name: code-review
description: 'Review code, walk a GitHub PR, or run a cleanup pass. Use when asked to review code, a PR or a range, on "review continue", or at delivery of a big diff.'
---

# Code review

Example. The owner types `/swarm:code-review` on a branch with open work. The reply asks one question: check
the range, walk a GitHub PR, or run a cleanup pass. The owner answers "walk 42", so the front door
plans from `Preset: code-review walk`, and the walk starts after the owner approves the plan.

The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.

## Modes

Pick the mode from the request. Ask the owner only when the request neither names nor implies a mode,
and not when the Activation section of `references/cleanup.md` starts cleanup at a delivery gate. The mode picks the preset.

| Mode | Runs | Preset and rules |
|---|---|---|
| `check` | a script verdict on a commit range or on local changes | [references/check.md](references/check.md) |
| `walk` | a GitHub PR walk with comments in the reviewer's voice in a pending review | [references/walk.md](references/walk.md) |
| `cleanup` | the six-step cleanup pass on work already written | [references/cleanup.md](references/cleanup.md); its Activation section owns when it runs |

More references, each read on its own trigger:

- [references/whole.md](references/whole.md): read when check mode starts the `whole` seat.
- [references/maintain.md](references/maintain.md): read when you add or change a rule, a lens, or a repo rules file.
- [references/cleanup-rules.md](references/cleanup-rules.md): read at cleanup steps 2 to 5, for the user's rules the global file lacks.
- [references/lens.md](references/lens.md): read when you add or change a rule; a review seat reads the run's `rules.md` instead.

Run the front door in `skills/swarm/SKILL.md` with `Preset: code-review <mode>`, then follow the mode
file. `review continue <run>` makes no new plan. It goes to the mode that owns that run folder
(`tmp/review-walk/` is walk, `tmp/review-check/` is check). Walk runs `status` on the run and takes
the first ready step, as `references/walk.md` says. Check runs `verdict <run>` again on the same run,
as the Asks section of `references/check.md` says. A caller that needs only a verdict, such as `implement` step 06 or the `change-review`
block, runs the check mode's script as `references/check.md` says, with no plan of its own.
