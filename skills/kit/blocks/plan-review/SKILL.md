---
name: plan-review
description: "Show the plan chart to the owner in Plannotator, change the plan for each note, repeat until they approve, then start the steps. Use when a plan is checked and before any step starts."
license: MIT
metadata:
  kind: step
  owns: "the owner's approval of a plan before work starts"
  serves: "plan"
  needs: "plan.md and plan.html"
  gives: "an approved plan and its started step run"
  conflicts: ""
  users: "plan"
  source: "blocks/comment-round/SKILL.md (one round, hashes, answers, loop stop); references/step_run.py (start)"
---

## Rules
- Run each round with the `comment-round` block on `<run>/plan.html <run>/plan.md`, with `<run>/plan-review` as its folder.
- After each round, act on the decision:
  - `approved` with no notes: the plan is approved.
  - `approved` with notes, or `annotated`: change `plan.md` for each note, answer each note, run `plan-chart`, and start another round. Any change to the plan needs a new round, because the approval holds only for the files the owner saw.
  - `dismissed`: ask the owner in chat.
- Start the work as step 4 of `skills/plan/SKILL.md` says. `start_plan.py` owns the stop rule, and its docstring says when a plan needs this round.
- An approved page or comment round elsewhere is never an approval of the plan.

## Example
Initial state: `plan.html` shows 03-tokens in stage 2.
Action: round 1 returns `annotated` with the note "skip this, the page has tokens". Remove 03-tokens, make 04-build need only `read`, answer the note, and draw the chart again. Round 2 returns `approved` with no notes.
Result: `start_plan.py <run>` makes the step files. Had the plan changed after round 2, it would print `../plan.md changed after round 2; start a new round` and make no step file.

## Check (script)
- `python3 -I <kit root>/blocks/plan-review/scripts/test_start_plan.py` prints `ok`: an owner-stop plan refuses with no round, with an unapproved round, and after an edit, and starts after a current approval; a chat preset starts with no round only when its steps and blocks are unchanged; a late, unknown, or wrong-mode `Preset:` line keeps the owner stop; a `Start:` preset prints its command.

## Check (judge)
- Was the step run started by `start_plan.py`, after the newest round? Quote the round file's decision and the start command's output.
