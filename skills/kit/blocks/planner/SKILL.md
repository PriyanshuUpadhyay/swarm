---
name: planner
description: "Turn a task into a plan file: a done-when line and a step table that names one or more blocks per step, what each step needs, and why. Use when any task starts, before work."
license: MIT
metadata:
  kind: step
  owns: "which blocks a task uses, in which order, with which inputs"
  serves: "plan"
  needs: "a task"
  gives: "plan.md, a step table with writes and blocks, and a Chosen blocks table"
  conflicts: ""
  users: "plan"
  source: "docs/blocks.md (The model: plan block, ledger, end condition); references/step-run.md (step table, Needs); blocks/requirements/SKILL.md (done-when claims)"
---

## Rules
- The kit's `references/plan-layout.md` owns how this file's schema fits the reply layout.
- Make the run folder by the kit's `references/run-folder.md`, and write `plan.md` in it.
- When a verb sent the task, start from that verb's preset, as `skills/plan/SKILL.md` defines it. Copy its table into `plan.md`, add the Writes column below, and write `Preset: <verb>` on the line under the title. Add a mode, as in `Preset: review walk`, only for a verb whose modes have their own presets. Add blocks or steps where the task needs them, but keep the steps and Needs of the implement preset, because `skills/implement/SKILL.md` says its `start` refuses any other. When you drop a preset step, add a line `Dropped steps: <step> (<reason>), ...` under the step table, so the owner sees it.
- When the preset has an `Offers:` line, read each offered block's line. Add it to a step when the task needs it. For each offered block that no step loads, add an `Offered` row to `## Blocks considered` with why it stays off, so the owner can say "add Y".
- List the blocks with `rg -N '^(name|description|  needs|  gives|  conflicts):' <kit root>/blocks/*/SKILL.md`. Pick from that list only. Read a block's full file only when its line does not settle the choice.
- `plan.md` is the page the owner reads, so it shows the work and keeps the plumbing in a fold. Its sections, in this order:
  1. `# Plan: <the task in one line>`, the `Preset:` line when a verb sent the task, one or two sentences on the result, and `Next action: <what the owner does>`.
  2. `## Questions this plan answers` as a numbered list (research), or `## The problem in one example` as one short paragraph with the initial state, the action, and the bad result (other tasks).
  3. `## Building blocks`: the few facts that decide the plan, each a `### Block N · <the fact>`. Its first line starts with `Known (<source>).` or `To check in step N.`. It may hold one diagram, and it ends with a `So in this plan:` line that names what the fact changes. Leave the section out when two or fewer steps need a brief, or when no fact changes a step.
  4. `## Steps`: one brief for each step between the first and the last, as `### N · <plain name>` and then a `Step: \`NN-word\`` line. A brief is bullets with the real work. A research brief has `Look in` (paths, sites), `- Seed searches:` with two or more `  - ` lines, and `Result:`. A build brief names each `file:line` it changes and `Result:`. Its last bullet is `Skill blocks:` with the step's blocks. `step_run.py start` copies each brief into its step file, so the worker gets the text the owner approved.
  5. `## Why this way, not another`: a table `| Choice | We do | Not this | Why |`, one row per real choice.
  6. `## Not doing`: bullets.
  7. `## Done when`: the line `Done when: <the result in one short line>`, one bullet per claim, then a line `Evidence:` and one bullet per proof. Choose that evidence by `blocks/requirements/SKILL.md`, and map results by `blocks/done-when-evidence/SKILL.md`. Write each claim and each proof as one short line, with no `(1) ... (2) ...` list inside a line.
  8. `<details>`, `<summary>How it runs (for the agent)</summary>`, the step table, the blocks table, then `</details>`.
- Each diagram has a line `*Answers: <the question it answers>*` right over it and at most 10 nodes. Use Mermaid; when Mermaid draws it badly, draw it in an `html` fence by `blocks/html-diagram/SKILL.md`.
- An implement or review plan keeps the older layout for now: `Done when:` under the title, the step table, and `## Blocks considered` with the blocks table. `skills/implement/SKILL.md` reads that layout.
- The step table has the header `| File | Needs | Holds | Writes | Block | Why |`. A plan that copies a `Stop: chat` preset with no change may keep the preset's header with no Writes.
  - File: `NN-<word>`, numbered in the order a reader follows. The word is what other steps write in Needs. Add ` (optional)` after the name of a step that `skip` may skip; `skip` refuses any other step.
  - Needs: the words of the steps whose output this step uses, or `none`. Steps with all needs done run in parallel, so add a need only for a real input.
  - Holds: what the step gives, in a few words.
  - Writes: the repo paths the step may change, comma-separated, a folder with a trailing `/`, or `none` for a step that writes only its run folder. Two steps that can run at the same time may not write the same path or one inside the other. `done` refuses a step that changed a path outside its own Writes and those of the other active steps.
  - Block: one or more block ids, comma-separated. The step's agent reads each `blocks/<id>/SKILL.md` and follows it.
  - Why: one short line (about 12 words) that says why this step is in the plan. Put detail in the step file, not in this cell.
- Each step with Needs `none` holds the `requirements` block.
- Each step with Writes has a step with the `change-review` block that reaches it through Needs. An agent that took a writing step cannot take that review step.
- The last step is the only step whose output no step uses. It holds a `voice-` block that fits the output: `voice-reply` for a reply to the owner, `voice-teach` when the owner wants to understand a subject, `voice-review` for review comments, and `voice-pr` for PR text.
- After the step table, add the blocks table with the header `| Block | Status | Why |`. It holds one `Chosen` row for each block, each with one reason, and one `Offered` row for each offered block that no step loads. `Chosen` is exactly the set of blocks that the steps load, after the parent and part expansion that `blocks/blocks.toml` sets. Sub-blocks are the parts of a parent block, as its `metadata.parts` names them. They have their own rows, and `plan-chart` shows them under their parent.
- Two blocks that name each other in `conflicts` cannot both be in the plan. Keep one, and say why in its Why cell.
- `Amend: <N>` on its own line after the step table lets a run add N steps that write `none` with `step_run.py amend`, with no new approval. Leave it out (N is 0) unless the owner sets it.
- When no block fits a step, write the step with the nearest block and say what is missing in Why. Do not invent a block id.
- Then run the `plan-chart` block, then the `plan-review` block. Work starts only after the owner approves.
- The owner reads the plan on the page that `plan-chart` draws, with each skill block and its description. The reply that asks for approval says in two or three lines what the plan does. The owner changes the plan with "drop X", "add Y", or "add sub-block Z". Change the steps, the step table, and the blocks table to match, run `plan-chart` again, and show the new page. Never start work on a changed plan that the owner has not seen.

## Example
Initial state: the research verb sends "can a call show the user's own number as caller ID?" in a repo that calls Twilio.
Action: write `plan.md` in the research run folder:

````
# Plan: find when a call can show the user's own number as caller ID

Preset: research

After this run you know which numbers Twilio lets us show, and what the user must do first.

Next action: approve, or annotate any card.

## Questions this plan answers

1. Which numbers can we set as caller ID? Answered in step 2.
2. Where does our code set it today? Answered in step 1.

## Building blocks

### Block 1 · Caller ID is one field on the outbound leg

Known (`src/voice/dial.ts:42` sets `callerId`). Twilio reads it when it places the call to the phone.

So in this plan: step 1 traces that one field, not the whole call.

## Steps

### 1 · Trace the field in our code

Step: `02-local`

- Look in `src/voice/` and `server/webhooks/`.
- Result: each place that sets `callerId`, with file and line.
- Skill blocks: `local-research`

### 2 · Check Twilio's rules

Step: `03-web`

- Look in Twilio's caller ID docs and its trial limits page.
- Seed searches:
  - `Twilio verified caller ID non-Twilio number`
  - `Twilio caller ID per subaccount`
- Result: the rule with a link to Twilio's own page.
- Skill blocks: `deep-search`, `primary-sources`, `cite-sources`

### 3 · Write the report

Step: `04-report`

- Sections: the answer, the rule with its link, what the user must do.
- Skill blocks: `report`, `link-check`

## Why this way, not another

| Choice | We do | Not this | Why |
|---|---|---|---|
| Source of the rule | Twilio's own docs | forum answers | forums repeat rules that changed |

## Not doing

- No code change.

## Done when

Done when: the report answers both questions with links.

- each question has an answer with a source

Evidence:
- the link check passes

<details>
<summary>How it runs (for the agent)</summary>

| File | Needs | Holds | Writes | Block | Why |
|---|---|---|---|---|---|
| 01-question | none | the two questions | none | requirements | the questions frame the run |
| 02-local | question | the code path | none | local-research | step 1 |
| 03-web | question | the rule with links | none | deep-search, primary-sources, cite-sources | step 2 |
| 04-report | local, web | the report path | none | report, link-check | step 3 |
| 05-close | report | the reply | none | voice-reply | the owner reads the answer |

| Block | Status | Why |
|---|---|---|
| requirements | Chosen | the questions say when the run is done |
| (one Chosen row for each other block that a step uses) | | |
| tool-trial | Offered | no tool is picked |

</details>
````

Result: the page shows the questions, one block, and three step cards with their searches. 02-local and 03-web run in parallel. `step_run.py start` gives `03-web.md` the brief "Check Twilio's rules" with its two searches.

## Check (script)
- `python3 -I <kit root>/blocks/plan-chart/scripts/plan_chart.py <run>/plan.md` exits 0: `Done when:` has a bullet per claim and an `Evidence:` bullet list, a research plan has a brief for each middle step, seed searches in its `deep-search` brief, a `So in this plan:` line in each building block, and an `Answers:` line over each diagram, every block exists, every Needs word names a step, the plan has no loop and one end, the end holds a `voice-` block, each step with Needs `none` holds `requirements`, each writing step is reached through Needs by a `change-review` step, no two parallel steps write the same path, and the blocks table lists each block once, its Chosen rows equal to the blocks the steps use, and any other row Offered.

## Check (judge)
- Does each brief name real places to look and real searches, not a block name again? Quote a brief that only restates its block.
- Does any block the steps do not use hold a rule that the task needs? Quote the rule and the task text.
- Is each claim and each proof one short line, and each Why cell one short line? Quote any that run long or hold a numbered list.
- Does each Why line give a reason that the task needs, and does any step lack a block that the task needs? Quote the Why line, or the task text that no step covers.
