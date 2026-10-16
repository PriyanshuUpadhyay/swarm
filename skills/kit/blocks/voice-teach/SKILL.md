---
name: voice-teach
description: "Explain a subject in learning order: start from a term the owner already used, then add one new idea at a time, each tied to the last. Use when the owner asks to understand something."
license: MIT
metadata:
  kind: rule
  owns: "the order of ideas in an explanation to learn from"
  serves: "close"
  needs: "the subject and the terms the owner already used"
  gives: "an explanation in learning order"
  conflicts: ""
  users: "planner, explain"
  source: "mined rows s2#37, s4#28; docs/council 2026-10-13 block rules (V3: one exit voice per kind of output, and 'understand' is one)"
---

## Rules
- Find the terms that the owner already used about the subject, in this chat or task. Start from one of them.
- Add one new idea per step. Tie each new idea to an idea already explained, and say how they connect.
- Keep one running example from start to end. Each new idea changes or extends that example.
- Gloss each new term in two or three words the first time it appears.
- Stop when the question is answered. Name the next idea in one line, and do not explain it.
- The session's reply rules still decide how the reply opens and how long it is. This block owns only the order of ideas.

## Example
Initial state: the owner asks how a database index works and has used the word "table".
Action: start from a table of 1 million users, then add a sorted list of emails (the index), then the search that halves the list each time, then the cost of keeping the list sorted on each insert.
Result: four steps on one example, each tied to the last, and the next idea, "a B-tree", named in one line.

## Check (script)
- none: the explanation goes to chat, and no file holds it.

## Check (judge)
- Does the explanation start from a term the owner used, and does each new idea name the earlier idea it builds on? Quote the starting term and two linked ideas.
