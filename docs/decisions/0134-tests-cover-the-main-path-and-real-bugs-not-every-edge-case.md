---
status: accepted
date: 2026-11-03
deciders: [user]
related: ["0133"]
informed-by:
  - 'research 2026-11-03 (~/.claude/reports/2026-11-03-agent-written-tests.md): 8-17% of LLM-written tests enforce a bug, and only 2.4-4.8% reveal one (arXiv 2610.11835); agents delete or weaken failing tests (arXiv 2510.20270)'
  - 'four challenger seats, 2026-11-03 (tmp/test-prune/tc-*.md on branch test-prune): 103 of 2,016 tests to remove and 44 to loosen; 244 commits named "review fix" or "round N"'
  - 'owner, 2026-11-03: "We would want to be able to write tests but we would not want to take care of each and every edge case in there." Then "go" on the picks in tmp/test-prune/analysis.md'
---

# 0134. Tests cover the main path and real bugs, not every edge case

## Context and Problem Statement

Agents write most code and tests in this repo. Each review round asked for a failing test per
finding (`check.md:134`), and `prove-it-works` asked for failure and boundary cases on every change.
So the kit had 8,849 test lines for 5,249 lines of script code. For example, three tests checked one
`gh` refusal, one per review round. Some tests waited 600 s, pinned whole sentences, or patched the
module they tested. Meanwhile CI ran no vitest and no kit tests.

## Considered Options

- A test rule owned by `blocks/prove-it-works/SKILL.md`, with the review rule limited to behavior
  bugs, and a prune of the tests that break it (chosen).
- Main-path tests only. The research says this lets bug-locking tests pile up, so real bugs and
  guards keep a failure test.
- No change. The test count keeps growing with each review round.

## Decision Outcome

Chosen option: the six-line test rule in `skills/kit/blocks/prove-it-works/SKILL.md`. One test per
behavior on its main path. A failure test only for a bug a user hit, or for a guard on auth, data
loss, a trust boundary, or the owner gate. A fix found only in a review round is an edge case. Tests
check outcomes, not wording or internals, add values to existing tests, use no real waits over
1 second, and are never edited to make a change pass. `check.md` gives a review ask a failing test
only under the same rule. CI runs `pnpm test`, `pnpm typecheck`, and `scripts/kit-checks.sh`.

### Consequences

- Good: fewer and faster tests, and CI runs the suites it skipped.
- Bad: an edge case found in review has no test, so it can come back. A review `note` records it.
- Open: a hook that blocks agent edits to existing test files needs its own decision.
