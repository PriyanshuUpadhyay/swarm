---
name: decision-with-default
description: "Ask about a real fork with your recommendation marked and keep consent with the owner. Use when a choice changes what gets built."
license: MIT
metadata:
  kind: rule
  owns: "when to ask about a real fork, its recommended default, and owner consent"
  serves: "contracts, design"
  needs: "a claim or step id"
  gives: "one answer line per decision"
  conflicts: ""
  users: "implement, engineering-standards"
  source: "html-plan pack: skills/html-plan/SKILL.md:80 (rule 6), :162-165 (answer endings), skills/html-plan/references/blocks.md:163-166 (doc-ask); references/plan-layout.md:14-15 (unanswered choice); skills/engineering-standards/SKILL.md section 5 (trade-off form, moved here 2026-10-13)"
---

## Rules
- A claim decision sits on the claim it changes, after that claim's exhibit and before its child claims.
- Ask only about forks that change what you build, at most 5 per plan. A plan with no such fork asks nothing.
- Ask a question only when the questions it depends on have answers. Word it so that a yes accepts the marked default.
- Mark the option you would pick. The marked option is your recommendation, so "I changed nothing" is a full answer. Never leave a choice with no default.
- When the task states the rule, the default is that rule as the frame's `Task rules:` line quotes it, by `blocks/requirements/SKILL.md`.
- If an option removes a claim, say so: "claim 4 goes".
- An answer of "not opened; default kept" is not agreement. If the decision is important, ask about it in chat.
- An unanswered choice is never treated as accepted.
- When a scope forks, two rules conflict, or a direction-changing choice remains open, explain the
  trade-off and stop the work that depends on it. Recommend an option and leave the decision with
  the human owner. A fork that a known fact settles needs no question.
- Give the relevant facts in concise prose or options, following the loaded session reply rules.
  Include what changes between the options and why you recommend one. Cost, affected users,
  reversibility, and evidence that could settle the choice are useful when they affect the decision;
  they are not fields to fill for every question.
- Record a decision once it is made, in the repo's decision log when it has one. This block writes no ADR.

## Example
Initial state: claim 2 says "A visitor sees how one request flows through `tracer`." The diagram could be static or step through on scroll. The fork is important: the scroll form adds a reduced-motion variant and its own checks to the build.
Action: put on claim 2:
"Should the request diagram animate on scroll?"
- (x) No, one static diagram (suggested)
- ( ) Yes, highlight each step on scroll. Adds a reduced-motion variant.
Result: the user answers "kept as proposed". The builder draws one static diagram. Had the line read "not opened; default kept", the builder asks once in chat before build, because the fork is important.

## Check (script)
- none: the decisions live only in the step file, which holds this block's own Example decision.

## Check (judge)
- Are there 5 claim decisions or fewer, and does each have exactly one `(x)` option? An open trade-off is not a claim decision. Quote each question with its options, or write "no fork".
- Does each claim decision sit on the claim it changes, after that claim's exhibit and before its child claims? Quote the decision and the claim.
- Does each open direction-changing choice state the relevant trade-off and recommendation, leave the decision with the human owner, and stop dependent work? Quote the choice and the stopped work.
- Does each "not opened" answer on an important claim decision have a follow-up question in chat, or a line that says why none was needed? Quote both.
