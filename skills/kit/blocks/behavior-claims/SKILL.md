# behavior-claims
Concept: The frame states what a reader can now do or see, as true-or-false claims, with done-when and the main risks.
Serves: frame
Source: html-plan pack: skills/html-plan/SKILL.md:20-30 (tree levels), :75-78 (rules 1, 2, 4), :87 (no visible behavior); skills/flow/SKILL.md (step table, 01-frame row); pm-skills pack: pm-execution/skills/pre-mortem/SKILL.md:21-25, :27-42, :44-46, :55-57, :84

## Rules
- Each level answers one question. Level 1: what can someone now do or see (a behavior). Level 2: how does that work (one entrypoint, rule, or record). Level 3: where (`file:line`).
- Split the top level by behavior. Never by file, layer, or order of work. Behavior is the one split the reader can judge without code.
- Every level-1 and level-2 claim is a sentence that can be true or false. 12 words at most. "A reader sees how the cache answers a request." Not "Cache section".
- The level-1 claims are the summary. Read them aloud. They must tell the whole change. Write no TL;DR, no sections list, no steps list.
- At most 5 children and 3 levels.
- If the change has no visible behavior, level 1 states the guarantees: "Nothing a caller sees changes."
- `01-frame.md` holds what the `01-frame.md` row of the step table in `skills/flow/SKILL.md` lists.
- Each done-when names the evidence that will prove it: a command, a screenshot, or a `file:line`.
- Pre-mortem: imagine the page ships in 14 days and fails. Ask what went wrong, what you missed, and what you were overconfident about.
- Sort each failure as a Tiger (a real risk, with evidence), a Paper Tiger (a concern that is overblown), or an Elephant (a risk that nobody is discussing or checking). When you are not sure which kind a failure is, call it a Tiger.
- A launch-blocking Tiger is one that must be solved before launch. Each launch-blocking Tiger gets one concrete mitigation. Each Elephant gets one step that investigates it before launch.

## Example
Initial state: the user asks for "a landing page for `tracer`, our CLI, with three diagrams".
Action: write `01-frame.md` with three level-1 claims:
1. A visitor learns what `tracer` does from the hero, in 20 words.
2. A visitor sees how one request flows through `tracer` in a diagram.
3. A visitor can copy the install command with one click.
Done-when: "Claim 2 diagram has the nodes of `docs/architecture.md:10-24`; evidence: each node cites its line there." Tiger: "The diagram shows a part the tool does not have." Mitigation: cite the source line for each node.
Result: read aloud, the three claims tell the whole page.

## Check (script)
- none: the frame writes only its step file, which holds this block's own Rules and Example.

## Check (judge)
- Does `01-frame.md` give the goal, the user, out of scope, done-when, and shared assumptions? Quote each field's line.
- Is each level-1 claim one sentence that can be false, in 12 words or fewer? Quote each claim with its word count.
- Do the level-1 claims, read alone, tell the whole change, and is no claim a file, a layer, or a step? Quote the claims.
- Does each done-when name the evidence that will prove it (a command, a screenshot, a `file:line`)? Quote each done-when.
- Is each failure sorted as a Tiger, a Paper Tiger, or an Elephant? Quote each failure with its kind.
- Does each launch-blocking Tiger have one concrete mitigation? Quote each launch-blocking Tiger and its mitigation.
- Does each Elephant have a step that investigates it before launch? Quote each Elephant and its step, or write "no Elephant".
