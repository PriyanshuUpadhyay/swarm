---
name: planner
description: "Turn a task into a plan file: a done-when line and a step table that names one or more blocks per step, what each step needs, and why. Use when any task starts, before work."
license: MIT
metadata:
  kind: step
  owns: "which blocks a task uses, in which order, with which inputs"
  serves: "plan"
  needs: "a task"
  gives: "plan.md, a step table with blocks and a Chosen/Rejected blocks table"
  conflicts: ""
  users: "plan"
  source: "docs/blocks.md (The model: plan block, ledger, end condition); references/step-run.md (step table, Needs); blocks/requirements/SKILL.md (done-when claims)"
---

## Rules
- The kit root holds `blocks/` and `references/`. It is two folders up from this file.
- Make the run folder `<repo-root>/tmp/plan/<YYYY-MM-DD>-<slug>/` by the kit's `references/run-folder.md`, and write `plan.md` in it.
- List the blocks with `rg -N '^(name|description|  needs|  gives|  conflicts):' <kit root>/blocks/*/SKILL.md`. Pick from that list only. Read a block's full file only when its line does not settle the choice.
- `plan.md` opens with `# Plan: <the task in one line>`, then `Done when: <the result, and the evidence that proves it>`. Choose that evidence by `blocks/requirements/SKILL.md`, and map results by `blocks/done-when-evidence/SKILL.md`.
- Then one table with the header `| File | Needs | Holds | Block | Why |`.
  - File: `NN-<word>`, numbered in the order a reader follows. The word is what other steps write in Needs.
  - Needs: the words of the steps whose output this step uses, or `none`. Steps with all needs done run in parallel, so add a need only for a real input.
  - Holds: what the step gives, in a few words.
  - Block: one or more block ids, comma-separated. The step's agent reads each `blocks/<id>/SKILL.md` and follows it.
  - Why: one line that says why this step is in the plan.
- The first step states the requirements (usually `requirements`) unless the task is one question.
- A plan whose steps make commits has a `change-review` step after the last build step.
- The last step is the only step whose output no step uses. It holds a `voice-` block that fits the output: `voice-reply` for a reply to the owner, `voice-teach` when the owner wants to understand a subject, `voice-review` for review comments, and `voice-pr` for PR text.
- After the step table, add `## Blocks considered` with the header `| Block | Status | Why |`. It holds one row for every catalog block except the base block, once, with Status `Chosen` or `Rejected` and one reason. `Chosen` is exactly the set of blocks that the steps load, after the parent and part expansion that `blocks/blocks.toml` sets. Sub-blocks are the members of an alias in `blocks/blocks.toml` `[aliases]`. They have their own rows, and `plan-chart` shows them under their alias.
- Two blocks that name each other in `conflicts` are alternatives. Keep one and say why in Why.
- When no block fits a step, write the step with the nearest block and say what is missing in Why. Do not invent a block id.
- Then run the `plan-chart` block, then the `plan-review` block. Work starts only after the owner approves.
- The reply that shows the plan carries the `### Chosen` and `### Rejected` tables that `plan-chart` prints, in full, before it asks for approval. The owner changes the plan with "drop X", "add Y", or "add sub-block Z". Change the step table and the blocks table to match, run `plan-chart` again, and show the new tables. Never start work on a changed plan that the owner has not seen.

## Example
Initial state: the task is "add dark mode to the settings page" in a repo with a theme file.
Action: write `tmp/plan/2026-10-13-dark-mode/plan.md`:

```
# Plan: add dark mode to the settings page

Done when: the settings page has a dark theme; evidence: screenshots at 375 px and 1280 px.

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-frame | none | done-when claims | requirements | the claims say when the work is done |
| 02-read | frame | findings with paths | local-research | find where the theme is set |
| 03-tokens | frame | DESIGN.md tokens | design-tokens | the dark colors need tokens |
| 04-build | read, tokens | the page | build-site, web-ui-basics | make the change |
| 05-shots | build | screenshots | headless-shot | the done-when needs screenshots |
| 06-reply | shots | the reply | voice-reply | the owner reads the result |

## Blocks considered

| Block | Status | Why |
|---|---|---|
| requirements | Chosen | the claims say when the work is done |
| headless-shot | Chosen | the done-when needs screenshots |
| html-diagram | Rejected | the page needs no diagram |
| story | Rejected | the task tells no story |
| (one row for each other catalog block) | | |
```

Result: 02-read and 03-tokens run in parallel, and 04-build waits for both. The owner sees every block as Chosen or Rejected, and answers "add sub-block html-diagram" to change one.

## Check (script)
- `python3 -I <kit root>/blocks/plan-chart/scripts/plan_chart.py <run>/plan.md` exits 0: every block exists, every Needs word names a step, the plan has no loop and one end, the end holds a `voice-` block, and the blocks table lists every catalog block once, with Chosen equal to the blocks the steps use.

## Check (judge)
- Does each Rejected reason say why the task does not need that block, and does any Rejected block hold a rule that the task needs? Quote the reason and the task text.
- Does each Why line give a reason that the task needs, and does any step lack a block that the task needs? Quote the Why line, or the task text that no step covers.
