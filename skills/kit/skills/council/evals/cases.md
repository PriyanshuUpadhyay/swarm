# council cases

Each case runs Round 1 only: three seats, one pinned brief, and the Round-1 output shape. The
skill side adds the shared rule and the seat's card from `references/roles.md`. Each proposal has
one false claim, and its pinned file proves the claim false.

## case: graph-store
Source: council 2026-11-05-graph-skill-blocks and owner, 2026-11-05: no graph store, no blocks compiled from task text.
Task: Judge this proposal. "Store the kit's 60 blocks and their Needs edges in a Neo4j graph, and let the planner build each task's plan by walking that graph at run time. The AIP paper supports this, because skills compiled from the task text passed 12 of 15 tasks. The cost is small, one Docker container."
Files: report.md (copy of `~/.claude/reports/2026-11-05-graph-skill-blocks.md`)
Check (script): none
Expect:
- The verdict is NO-GO, or GO-WITH-CHANGES that drops the graph store.
- At least one seat says the 12 of 15 claim is wrong, and quotes the report's 0/15.
- At least one seat names a concrete cost or failure of a graph store, such as a new service to keep running or a graph that drifts from the files.
- The positions name a cheaper first step that tests whether a graph is needed.

## case: full-flow-always
Source: trial 5 (`~/.swarm/ws/trial5/RESULT.md`) and ADR 0133: a small change takes a short route.
Task: Judge this proposal. "Every code change, even a 35-line CLI flag, runs the full implement flow (frame, design, contracts, impact, build, review, close) with six seats, because trial 5 showed that the full kit caught bugs that the run with no kit missed."
Files: RESULT.md (copy of `~/.swarm/ws/trial5/RESULT.md`)
Check (script): none
Expect:
- The verdict rejects the full flow for small changes: NO-GO, or GO-WITH-CHANGES with a short route.
- At least one seat says the "caught bugs" claim is wrong, and quotes the hidden tests at 8/8 for all three arms or the equal scores.
- At least one seat names the time cost, such as about 1.5 min against about 15.5 min, or more than 3x.
- The positions name a size rule or a short route for small changes.
