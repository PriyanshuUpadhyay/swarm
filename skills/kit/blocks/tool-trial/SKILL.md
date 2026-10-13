---
name: tool-trial
description: "Pick a tool, library, or service by trying at least three on this machine on the same real task, and record time and result. Use when a question or a design picks a tool for later use."
license: MIT
metadata:
  kind: step
  owns: "the hands-on test of tool candidates"
  serves: "research, contracts"
  needs: "a tool question and a real task"
  gives: "a table of candidates with install, task, time, and result"
  conflicts: ""
  users: "research"
  source: "skills/research/SKILL.md (04-test, moved here 2026-10-13)"
---

## Rules
- Make a shortlist of at least three candidates.
- Install each one in a scratch place, run the same real task on each, and record the install, the time, and the result. Remove the ones you do not pick.
- A candidate that is not installed stays on the list, because install time is not a reason to drop it.
- A slower, tested answer beats a fast pick from reading alone.
- Ask before an install that changes state outside a scratch place, such as a GUI permission or a system service.

## Example
Initial state: the question is "which tool turns this Markdown report into a PDF".
Action: install three candidates in `/tmp/pdf-trial/`, convert the same report with each, and time it.
Result: a table with three rows, install, task, time, and result; the two tools not picked are removed from the scratch place.

## Check (script)
- none: the trial runs in a scratch place outside the commit range.

## Check (judge)
- Does the table hold three or more candidates, each run on the same task with its time and result? Quote the table.
- Were the candidates not picked removed, and was any install outside a scratch place asked about first? Quote the lines.
