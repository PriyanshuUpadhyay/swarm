---
name: voice
description: "Keep the reply-rule pointer that the voice blocks share with each selected voice part. Use when a step writes a reply, an explanation, PR text, or review comments."
license: MIT
metadata:
  kind: parent
  owns: "shared voice rules"
  serves: "close, review"
  needs: "the kind of output"
  gives: "shared rules for the selected voice parts"
  conflicts: ""
  users: "planner, explain"
  source: "blocks/voice-reply/SKILL.md (the reply rules live in the loaded agent instructions, not restated); blocks/voice-teach/SKILL.md (the session's reply rules decide how the reply opens and how long it is)"
  parts: "voice-pr, voice-reply, voice-review, voice-teach"
---

## Rules
- The owner's reply rules live in the agent instructions that the session loaded (`AGENTS.md`, `CLAUDE.md`). They decide how a reply opens and how long it is. A voice block does not restate them.

Each part keeps its own checks.

## Example
Initial state: the last step holds `voice-teach`, and the loaded `AGENTS.md` says to lead with the result.
Action: open the explanation with the answer, as `AGENTS.md` says, then add the ideas in the order that `voice-teach` sets.
Result: the opening and the length come from `AGENTS.md`, the order of ideas comes from `voice-teach`, and no block copies a reply rule.

## Check (script)
- none: the reply goes to chat, and no file holds it.

## Check (judge)
- none: each part's judge check compares its own output with the reply rules or the voice file.
