---
name: voice-reply
description: "Write the final answer to the owner by the reply rules of the session's agent instructions, with every failed or skipped step named. Use when the last step of a plan writes to the owner."
license: MIT
metadata:
  kind: rule
  owns: "the final reply to the owner from the plan's results"
  serves: "close"
  needs: "the results of the plan's steps"
  gives: "the reply"
  conflicts: ""
  users: "plan"
  source: "the owner's agent instructions (reply rules); docs/council 2026-10-13 block rules (N4: a pointer, not a copy of those rules)"
---

## Rules
- Apply the owner's reply rules to the final answer.
- When the session loaded no such file, say so in the reply, and write in plain words.
- Take each result from its step file. Name each step that failed or was skipped, with its output. Never round a partial result up to done.

## Example
Initial state: the steps found that the theme lives in `src/theme.ts` and added dark tokens, and the 375 px screenshot shows a white header.
Action: write the reply by the loaded reply rules.
Result: the reply says that dark mode works except the header at 375 px, and names that open item with the screenshot path.

## Check (script)
- none: the reply goes to chat, and no file holds it.

## Check (judge)
- Does the reply follow each reply rule of the loaded agent instructions, and name each failed or skipped step? Quote one rule with the reply line that meets it, and each failed or skipped step.
