---
name: planner
description: "Turn a task into a plan file: a done-when line and a step table that names one or more blocks per step, what each step needs, and why. Use when any task starts, before work."
license: MIT
metadata:
  kind: step
  owns: "which blocks a task uses, in which order, with which inputs"
  serves: "plan"
  needs: "a task"
  gives: "plan.md, a step table with writes and blocks, and a Chosen/Rejected blocks table"
  conflicts: ""
  users: "plan"
  source: "docs/blocks.md (The model: plan block, ledger, end condition); references/step-run.md (step table, Needs); blocks/requirements/SKILL.md (done-when claims)"
---

## Rules
- The kit's `references/plan-layout.md` owns how this file's schema fits the reply layout.
- Make the run folder by the kit's `references/run-folder.md`, and write `plan.md` in it.
- When a verb sent the task, start from that verb's preset, as `skills/plan/SKILL.md` defines it. Copy its table into `plan.md`, add the Writes column below, and write `Preset: <verb>` on the line under the title. Add a mode, as in `Preset: review walk`, only for a verb whose modes have their own presets. Add blocks or steps where the task needs them, but keep the steps and Needs of the implement preset, because `skills/implement/SKILL.md` says its `start` refuses any other. When you drop a preset step, add a line `Dropped steps: <step> (<reason>), ...` under the step table, so the owner sees it.
- List the blocks with `rg -N '^(name|description|  needs|  gives|  conflicts):' <kit root>/blocks/*/SKILL.md`. Pick from that list only. Read a block's full file only when its line does not settle the choice.
- `plan.md` opens with `# Plan: <the task in one line>`, then `Done when: <the result in one short line>`. After it, one bullet per claim, then a line `Evidence:` and one bullet per proof. Choose that evidence by `blocks/requirements/SKILL.md`, and map results by `blocks/done-when-evidence/SKILL.md`. Write each claim and each proof as one short line, with no `(1) ... (2) ...` list inside a line.
- Then one table with the header `| File | Needs | Holds | Writes | Block | Why |`. A plan that copies a `Stop: chat` preset with no change may keep the preset's header with no Writes.
  - File: `NN-<word>`, numbered in the order a reader follows. The word is what other steps write in Needs. Add ` (optional)` after the name of a step that `skip` may skip; `skip` refuses any other step.
  - Needs: the words of the steps whose output this step uses, or `none`. Steps with all needs done run in parallel, so add a need only for a real input.
  - Holds: what the step gives, in a few words.
  - Writes: the repo paths the step may change, comma-separated, a folder with a trailing `/`, or `none` for a step that writes only its run folder. Two steps that can run at the same time may not write the same path or one inside the other. `done` refuses a step that changed a path outside its own Writes and those of the other active steps.
  - Block: one or more block ids, comma-separated. The step's agent reads each `blocks/<id>/SKILL.md` and follows it.
  - Why: one short line (about 12 words) that says why this step is in the plan. Put detail in the step file, not in this cell.
- Each step with Needs `none` holds the `requirements` block.
- Each step with Writes has a step with the `change-review` block that reaches it through Needs. An agent that took a writing step cannot take that review step.
- The last step is the only step whose output no step uses. It holds a `voice-` block that fits the output: `voice-reply` for a reply to the owner, `voice-teach` when the owner wants to understand a subject, `voice-review` for review comments, and `voice-pr` for PR text.
- After the step table, add `## Blocks considered` with the header `| Block | Status | Why |`. It holds one `Chosen` row for each block, and one `Rejected` row for each block that a Chosen block names in `conflicts`, each with one reason. `Chosen` is exactly the set of blocks that the steps load, after the parent and part expansion that `blocks/blocks.toml` sets. Sub-blocks are the parts of a parent block, as its `metadata.parts` names them. They have their own rows, and `plan-chart` shows them under their parent.
- Two blocks that name each other in `conflicts` are alternatives. Keep one and say why the other is Rejected.
- `Amend: <N>` on its own line after the step table lets a run add N steps that write `none` with `step_run.py amend`, with no new approval. Leave it out (N is 0) unless the owner sets it.
- When no block fits a step, write the step with the nearest block and say what is missing in Why. Do not invent a block id.
- Then run the `plan-chart` block, then the `plan-review` block. Work starts only after the owner approves.
- The reply that shows the plan carries the `### Chosen` and `### Rejected` tables that `plan-chart` prints, in full, before it asks for approval. The owner changes the plan with "drop X", "add Y", or "add sub-block Z". Change the step table and the blocks table to match, run `plan-chart` again, and show the new tables. Never start work on a changed plan that the owner has not seen.

## Example
Initial state: the task is "add dark mode to the settings page" in a repo with a theme file.
Action: write `tmp/plan/2026-10-13-dark-mode/plan.md`:

```
# Plan: add dark mode to the settings page

Done when: the settings page has a dark theme.

- the page shows dark colors from `tokens.css`
- the theme follows the system setting

Evidence:
- screenshots at 375 px and 1280 px
- a `vitest` case for the system setting

| File | Needs | Holds | Writes | Block | Why |
|---|---|---|---|---|---|
| 01-frame | none | done-when claims | none | requirements | the claims say when the work is done |
| 02-read | frame | findings with paths | none | local-research | find where the theme is set |
| 03-tokens | frame | DESIGN.md tokens | `DESIGN.md` | design-tokens | the dark colors need tokens |
| 04-build | read, tokens | the page | `web/src/` | build-site, web-ui-basics | make the change |
| 05-shots | build | screenshots | none | headless-shot | the done-when needs screenshots |
| 06-review | shots | the verdict | none | change-review | new seats check the change |
| 07-reply | review | the reply | none | voice-reply | the owner reads the result |

## Blocks considered

| Block | Status | Why |
|---|---|---|
| requirements | Chosen | the claims say when the work is done |
| headless-shot | Chosen | the done-when needs screenshots |
| (one Chosen row for each other block that a step uses) | | |
```

Result: 02-read and 03-tokens run in parallel, and 04-build waits for both. Only 03-tokens writes, so 02-read cannot change its files. The owner sees the Chosen blocks, and answers "add sub-block html-diagram" to change one.

## Check (script)
- `python3 -I <kit root>/blocks/plan-chart/scripts/plan_chart.py <run>/plan.md` exits 0: `Done when:` has a bullet per claim and an `Evidence:` bullet list, every block exists, every Needs word names a step, the plan has no loop and one end, the end holds a `voice-` block, the first step holds `requirements`, each writing step has a later `change-review`, no two parallel steps write the same path, and the blocks table lists each block once, with Chosen equal to the blocks the steps use and Rejected equal to their `conflicts` alternatives.

## Check (judge)
- Does each Rejected reason say why the task does not need that block, and does any block the steps do not use hold a rule that the task needs? Quote the reason and the task text.
- Is each claim and each proof one short line, and each Why cell one short line? Quote any that run long or hold a numbered list.
- Does each Why line give a reason that the task needs, and does any step lack a block that the task needs? Quote the Why line, or the task text that no step covers.
