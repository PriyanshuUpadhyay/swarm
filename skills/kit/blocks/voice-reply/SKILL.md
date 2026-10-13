---
name: voice-reply
description: "Rewrite the final answer to the owner in their reply style: result first, one example, then the rule, in plain words. Use when the last step of a plan writes to the owner."
license: MIT
metadata:
  kind: rule
  owns: "how the final reply to the owner sounds"
  serves: "close"
  needs: "the results of the plan's steps"
  gives: "the reply"
  conflicts: ""
  users: "plan"
  source: "references/explain-formats.md (example first); references/plan-layout.md (first screen of a long result)"
---

## Rules
- The owner's own reply rules win. Read them from the agent instructions that the session loaded (`AGENTS.md`, `CLAUDE.md`). Use the rules below only where those say nothing.
- Lead with the result or the next action. No preamble, no recap, no closing line.
- Explain with one concrete example first: initial state, action, result. Then give the rule in one line.
- Use plain, common words. Gloss a needed technical term in two or three words the first time.
- One idea per sentence. Short paragraphs.
- Say what is not verified and what failed, with its output. Never round a partial result up to done.
- A long result goes in the reply in full and opens with the first screen of the kit's `references/plan-layout.md`.

## Example
Initial state: the steps found that the theme lives in `src/theme.ts` and added dark tokens, and the 375 px screenshot shows a white header.
Action: write the reply.
Result: "Dark mode works on the settings page, except the header, which stays white at 375 px. For example, …" Then one line with the rule, then the open item.

## Check (script)
- none: the reply goes to chat, and no file holds it.

## Check (judge)
- Does the reply open with the result, give one example before the rule, and name each unverified item? Quote the first sentence and each unverified item.
