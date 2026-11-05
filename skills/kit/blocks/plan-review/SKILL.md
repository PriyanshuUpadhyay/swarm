---
name: plan-review
description: "Show the plan chart to the owner in Plannotator, change the plan for each note, repeat until they approve, then start the steps. Use when a plan is checked and before any step starts."
license: MIT
metadata:
  kind: step
  owns: "the owner's approval of a plan before work starts"
  serves: "swarm"
  needs: "plan.html drawn from plan.md"
  gives: "an approved plan and its started step run"
  conflicts: ""
  users: "swarm"
  source: "blocks/comment-round/SKILL.md (one round, hashes, answers, loop stop); references/step_run.py (start)"
---

## Rules
- Run each round with the `comment-round` block on `<run>/plan.html` only, with `<run>/plan-review` as its folder. The page holds all of `plan.md`, and the page carries the hash of the `plan.md` it was drawn from, so `start_plan.py` refuses a `plan.md` edited after the round, so the owner never reads the same plan twice.
- After each round, act on the decision:
  - `approved` with no notes: the plan is approved.
  - `approved` with notes, or `annotated`: change `plan.md` for each note, answer each note, run `plan-chart`, and start another round. Any change to the plan needs a new round, because the approval holds only for the files the owner saw.
  - `dismissed`: ask the owner in chat.
- Start the work as step 4 of `skills/swarm/SKILL.md` says. `start_plan.py` owns the stop rule, and its docstring says when a plan needs this round.
- When the owner approves in words instead of on the page, save their message to a file, add the line `Auto approval: "<their exact words>"` to `plan.md`, run `plan-chart`, then run `start_plan.py <run> --auto <file>`. The script refuses words that are not in the file and a page drawn from an older `plan.md`. Never write the words yourself.
- An approved page or comment round elsewhere is never an approval of the plan.
- When the owner asks for a design review, or the plan adds a public interface, run the vendor EGM plugin's `egm-review` (`/egm:egm-review` in Claude Code and AGY, the EGM skill in Codex) on `plan.md` before round 1. Its fresh reviewers read the plan with no history. Fix each finding the plan owns, and put the rest in the plan's open questions. EGM is installed from `google/elephant-goldfish-model` (Apache-2.0) and never edited. When it is not installed, say so and go on.

## Example
Initial state: `plan.html` shows 03-tokens in stage 2.
Action: round 1 returns `annotated` with the note "skip this, the page has tokens". Remove 03-tokens, make 04-build need only `read`, answer the note, and draw the chart again. Round 2 returns `approved` with no notes.
Result: `start_plan.py <run>` makes the step files. Had `plan.md` changed after round 2, it would print `plan.md changed after the owner approved plan.html; run plan-chart and start a new round` and make no step file.

## Check (script)
- `python3 -I <kit root>/blocks/plan-review/scripts/test_start_plan.py` prints `ok`: an owner-stop plan refuses with no round, with an unapproved round, after an edit to `plan.md`, and after an edit to `plan.html`, and starts after a current approval; a chat preset starts with no round only when every row that the start writes (File, Needs, Holds, Block) is the preset's own, so a decoy table or a changed Holds needs the round; a late, unknown, or wrong-mode `Preset:` line keeps the owner stop; a `Start:` preset prints its command; a re-approved plan whose existing steps changed their needs, holds, writes, blocks, or brief, or dropped a step, refuses and writes no file, while an unchanged plan, a taken step, an added step, a repo `[off]` block, and a kit update alone (a part it adds, a block it no longer has) still start, while a resolver error stops the start with its reason.

## Check (judge)
- Was the step run started by `start_plan.py`, after the newest round? Quote the round file's decision and the start command's output.
