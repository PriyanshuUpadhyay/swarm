---
name: change-review
description: "Run the review check mode whole on the commits that a plan's build steps made, and pass on the verdict its script printed. Use when a plan made commits that need a review verdict."
license: MIT
metadata:
  kind: step
  owns: "that a built change gets the full review check verdict"
  serves: "review"
  needs: "the commit range that the build steps made"
  gives: "the verdict that review_check.py printed"
  conflicts: ""
  users: "planner"
  source: "skills/review/references/check.md (Contract, Run); docs/council 2026-10-13 block rules (R-f: a plan needs a block that runs the full review)"
---

## Rules
- The kit root is two folders up from this file. Read `<kit root>/skills/review/references/check.md` and run it whole on `<base>..<head>`: the commit before the first build step, up to the last build commit. This block adds no rule, lens, or verdict of its own.
- The session that wrote the change never fills the checklist. The rows come from new seats, as that file says, started by the `run-workers` block.
- Pass on only the verdict line that `review_check.py verdict` printed. `INCOMPLETE` is not a pass.
- On `REQUEST CHANGES`, fix each finding as its own unit by the `checked-units` block, then review the new range again.

## Example
Initial state: the build steps made the commits `a1b2c3d..HEAD`.
Action: run the check on that range. The verdict prints `REQUEST CHANGES` with `C-7`. Fix it in one commit and run the review again.
Result: the second verdict prints `APPROVE`, and the step quotes that line and its range.

## Check (script)
- `sed -n 3p <repo-root>/tmp/review-check/<review-run>/03-verdict.md` starts with `Verdict: APPROVE`: the newest review approved the range.

## Check (judge)
- Does the step quote the verdict line as the script printed it, for a range that ends at the last build commit? Quote the line, the range, and `git log -1 --format=%h`.
