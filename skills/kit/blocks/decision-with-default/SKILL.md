---
name: decision-with-default
description: "Ask about a real fork as one short question with your pick marked, or write the trade-off form when a choice is open. Use when a choice changes what gets built."
license: MIT
metadata:
  kind: rule
  owns: "a real fork asked with a default, and the form of an open trade-off"
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
- Mark the option you would pick. The marked option is your recommendation, so "I changed nothing" is a full answer. Never leave a choice with no default.
- If an option removes a claim, say so: "claim 4 goes".
- The question is 15 words at most.
- An answer of "not opened; default kept" is not agreement. If the decision is important, ask about it in chat.
- An unanswered choice is never treated as accepted.
- When a scope forks, two rules conflict, or the question is open, write the trade-off form and stop. It recommends and never rules. Write it only for a trade-off that is really open, not for a fork that a known fact settles.

  ```text
  Fork:            <the choice, in one line>
  Options:         <two or three, named>
  Cost of each:    <what each one buys and what it spends>
  Who is hurt:     <which caller, user, or operator feels it>
  Reversibility:   <cheap to undo, or one-way>
  What would settle it: <the measurement or fact that decides it>
  Recommendation:  <one option, with the reason>
  Owner:           human
  ```
- Record a decision once it is made with the `decisions` skill. This block writes no ADR.

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
- Are there 5 claim decisions or fewer, and does each have exactly one `(x)` option? A trade-off form is not a claim decision. Quote each question with its options, or write "no fork".
- Does each claim decision sit on the claim it changes, after that claim's exhibit and before its child claims? Quote the decision and the claim.
- Is each claim decision's question 15 words or fewer? Quote each question with its word count.
- Does each "not opened" answer on an important claim decision have a follow-up question in chat, or a line that says why none was needed? Quote both.
