---
name: plan-review
description: "Show the plan chart to the owner in Plannotator, take their notes, change the plan, and repeat until they approve. Use when a plan is checked and before any step starts."
license: MIT
metadata:
  kind: step
  owns: "the owner's approval of a plan before work starts"
  serves: "plan"
  needs: "plan.md and plan.html"
  gives: "an approved plan and one result file per round"
  conflicts: ""
  users: "plan"
  source: "docs/blocks.md (annotate-loop is strict; loop stop); plannotator 0.28.17 `annotate --help` and its JSON result {v, surface, decision, message, noop, annotationCount}"
---

## Rules
- First run `command -v plannotator`. When it fails, write `Status: unavailable plannotator` in `<run>/plan-review.md`, tell the owner, and stop. Never skip the review.
- Round N: run `plannotator annotate <run>/plan.html <run>/plan.md --gate --json --result-file <run>/plan-review/<N>.json`. It opens the chart and the plan as one review. It waits for the owner, so start it where a tool time limit cannot stop it, and read the result file when it appears.
- Read `decision` from the JSON, never the message text.
  - `approved` with an empty `message`: the plan is approved.
  - `approved` with notes: make each change. If a change touches the table, run `plan-chart` and start another round. Otherwise the plan is approved.
  - `annotated`: change `plan.md` for each note, run `plan-chart`, and start another round.
  - `dismissed`: ask the owner in chat. A closed page is never an approval.
- The page closes on submit and shows no replies. So write your answer to each note in `<run>/plan-review/<N>-answers.md`, below a quote of the note.
- After 3 rounds with no approval, stop and ask the owner in chat which notes are still open.
- When the plan is approved, run `python3 -I <kit root>/references/step_run.py start <run> <run>/plan.md`. The kit root is two folders up from this file. Then run each ready step by the kit's `references/step-run.md`.

## Example
Initial state: `plan.html` shows 03-tokens in stage 2.
Action: round 1 opens Plannotator. The owner marks 03-tokens with "skip this, the page has tokens" and submits. `1.json` holds `"decision": "annotated"`.
Result: the step goes, 04-build needs only `read`, the chart is drawn again, and `1-answers.md` says "removed 03-tokens; 04-build now needs only 02-read". Round 2 returns `"decision": "approved"` and an empty message, so `step_run.py start` makes the step files.

## Check (script)
- `jq -r .decision <run>/plan-review/$(ls <run>/plan-review | rg '^\d+\.json$' | sort -n | tail -1)` prints `approved`: the newest round is an approval.

## Check (judge)
- Does each note in the round files have an answer in its answers file that names the change made? Quote the note and the answer.
