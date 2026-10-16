---
name: task-file
description: "Keep one small task file per task, so work survives a /clear, a compact, or a resume, and start the first reply after one with a status block. Use when a skill runs a task over many turns."
license: MIT
metadata:
  kind: rule
  owns: "the task file of a multi-turn task and the resume from it"
  serves: "build"
  needs: "a task that spans turns"
  gives: "a task file and a status block on resume"
  conflicts: ""
  users: "pair, deliver"
  source: "skills/pair/SKILL.md (One task file per task, moved here 2026-10-13); skills/deliver/SKILL.md step 4; references/run-folder.md (where the folder is)"
---

## Rules
- Keep one task file per task at `<repo-root>/tmp/<skill>/<YYYY-MM-DD>-<slug>.md`, as the kit's `references/run-folder.md` says. `tmp/` stays in `.git/info/exclude`, so git never tracks the file. Keep no second plan file.
- The file holds the goal in one line, the steps or behaviors in order, the current one, one evidence line per finished one (the check that ran and its result, or the run that showed it on the real surface), the commit of each finished one, the checks to run, and the next step. The skill that uses this block names any extra field.
- Update the file after every finished step.
- Read the file first after a `/clear`, a `/compact`, or a resume. Make the first reply a status block: what is done with its commits, what is open, the current step, and the open decisions, then the next step. Redo nothing that is done. The file is the truth, not the chat.
- The file is a log for a resume. Keep it to its fields, with no source survey and no essay. Apply the loaded agent instructions to user-facing output.
- Give other agents the file's absolute path, never a summary of it.

## Example
Initial state: a deliver run has landed units 1-3, and the session is compacted.
Action: read `tmp/deliver/2026-10-13-login.md` first. The first reply is a status block: "Done: U1 a1b2c3d, U2 d4e5f6a, U3 b7c8d9e. Open: U4 token refresh. Next: write the failing refresh test."
Result: the run goes on at unit 4, and no landed unit is redone.

## Check (script)
- none: the task file sits in `tmp/`, outside the commit range.

## Check (judge)
- Does the task file hold one evidence line for each finished step? Quote two of them.
- After a resume, was the first reply a status block with done, open, current, and next? Quote it, or write "no resume".
