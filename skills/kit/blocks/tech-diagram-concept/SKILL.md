# tech-diagram-concept
Concept: A diagram is drawn only when the subject has a shape text cannot hold, and its type and size follow that shape.
Serves: design, build
Source: references/explain-formats.md:7-8, :16-27, :36-37; html-plan pack: skills/html-plan/references/blocks.md:79-83 (doc-machine), :188, :192-193 (doc-flow), skills/html-plan/SKILL.md:119 (caption)

## Rules
- Draw a diagram when the subject calls across three or more parts in an order that matters; adds, removes, or reorders a branch, a guard, or an async step; moves through a lifecycle, a status, or a retry flow; or changes a schema or a data model.
- Skip it for one file, a rename, a config value, or a list. Text already shows those.
- Pick the type from the shape: a sequence for calls over time, a flowchart for decisions, a state diagram for a lifecycle, an entity diagram for a schema.
- For each diagram, write in `02-design.md` the trigger it meets and the type picked from the shape.
- Every branch leads somewhere, so the diagram also shows the edge cases.
- A parts diagram (the parts of a system and how they connect, not the steps of a flow or a sequence) has 12 nodes at most, in 2 to 3 columns. Place the nodes on a grid.
- A state diagram has 8 states at most. Put the main path on the top row and the ways out below it. Without a grid, labels overlap once arrows cross. No state that cannot be reached. No dead end that is not final.
- An arrow label is a key, a code, or two words.
- A node or an arrow that names code carries its `path:line` (in the page source as a comment or `data-src`).
- The caption is one sentence: what to notice.

## Example
Initial state: claim 2 needs "how one request flows through `tracer`". The source `docs/architecture.md:10-24` names CLI, agent, collector, and store, with 3 retries when the collector is down, after which the span is dropped and logged.
Action: the shape is calls over time, so draw a sequence: 4 lanes, 5 arrows, plus a `retry x3` branch that ends at "dropped, logged". Caption: "When the collector is down, a span gets 3 retries and is then dropped and logged." Each node and the caption trace to that source line. `02-design.md` records the trigger (calls across 4 parts in an order that matters) and the type (sequence).
Result: one diagram with 5 nodes (4 lanes and "dropped, logged") and every branch ending at a node.

## Check (script)
- none: what counts as a node depends on each diagram's markup, so the node count is a judge check.

## Check (judge)
- Does each parts diagram have 12 nodes or fewer, and each state diagram 8 states or fewer (other types have no node limit)? Quote each diagram's type, its node names, and their count.
- For each diagram, does `02-design.md` name the trigger it meets and the type picked from the shape? Quote both.
- Does each branch end at a node or a final state? Quote each branch and its end.
- Is each caption one sentence that says what to notice? Quote the caption.
