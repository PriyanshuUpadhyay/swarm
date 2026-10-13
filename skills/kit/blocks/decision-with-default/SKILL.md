# decision-with-default
Concept: A real fork is asked as a decision on the claim it changes, with your recommendation pre-picked.
Serves: contracts, design
Source: html-plan pack: skills/html-plan/SKILL.md:80 (rule 6), :162-165 (answer endings), skills/html-plan/references/blocks.md:163-166 (doc-ask); references/plan-layout.md:14-15 (unanswered choice)

## Rules
- A decision sits on the claim it changes, after that claim's exhibit and before its child claims.
- Ask only about forks that change what you build. Ask 2 to 5 per plan.
- Mark the option you would pick. The marked option is your recommendation, so "I changed nothing" is a full answer. Never leave a choice with no default.
- If an option removes a claim, say so: "claim 4 goes".
- The question is 15 words at most.
- An answer of "not opened; default kept" is not agreement. If the decision is important, ask about it in chat.
- An unanswered choice is never treated as accepted.

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
- Are there 2 to 5 decisions, and does each have exactly one `(x)` option? Quote each question with its options.
- Does each decision sit on the claim it changes, after that claim's exhibit and before its child claims? Quote the decision and the claim.
- Is each question 15 words or fewer? Quote each question with its word count.
- Does each "not opened" answer on an important decision have a follow-up question in chat, or a line that says why none was needed? Quote both.
