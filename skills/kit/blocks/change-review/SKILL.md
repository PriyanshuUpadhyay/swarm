---
name: change-review
description: "Run the review check on the commits that a plan's build steps made, or review a piece's written files outside any repo, and pass on the verdict. Use when a plan wrote work that needs a review verdict."
license: MIT
metadata:
  kind: step
  owns: "that a built change gets the full review check verdict"
  serves: "review"
  needs: "the commit range that the build steps made"
  gives: "the verdict that review_check.py printed"
  conflicts: ""
  users: "planner"
  source: "presets/review/references/check.md (Contract, Run); docs/council 2026-10-13 block rules (R-f: a plan needs a block that runs the full review)"
---

## Rules
- The kit root is two folders up from this file. Read `<kit root>/presets/review/references/check.md` and run it whole on `<base>..<head>`: the commit before the first build step, up to the last build commit. This block adds no rule, lens, or verdict of its own.
- The session that wrote the change never fills the checklist. The rows come from new seats, as that file says, started by the `run-workers` block.
- Pass on only the verdict line that `review_check.py verdict` printed. `INCOMPLETE` is not a pass.
- On `REQUEST CHANGES`, fix each finding as its own unit by the `checked-units` block, then review the new range again.
- When `git -C <repo-root> rev-parse` finds no work tree, the piece has no commits, so new seats review its files instead. Each seat writes rows to `<run>/file-review.md`, one for each written file and each done-when claim of the frame, with `pass` or `fix` and its evidence: a rendered frame, a command output, or a `file:line`. Fix each `fix` and review again. The last seat to finish writes the one last line, `Verdict: APPROVE`, only when it finds no `fix` row. Inside a repo, a file review never replaces the commit review.

## Example
Initial state: the build steps made the commits `a1b2c3d..HEAD`.
Action: run the check on that range. The verdict prints `REQUEST CHANGES` with `C-7`. Fix it in one commit and run the review again.
Result: the second verdict prints `APPROVE`, and the step quotes that line and its range. For a loop outside any repo, two seats write `file-review.md` rows for `index.html`, `loop.mp4`, and each claim, and its last line is `Verdict: APPROVE`.

## Check (script)
- `sed -n 3p <repo-root>/tmp/review-check/<review-run>/03-verdict.md` starts with `Verdict: APPROVE`: the newest review approved the range.
- `git -C <repo-root> rev-parse --is-inside-work-tree >/dev/null 2>&1 || test "$(tail -n 1 <run>/file-review.md)" = 'Verdict: APPROVE'` exits 0: outside any repo, the file review approved the written files.

## Check (judge)
- Does the step quote the verdict line as the script printed it, for a range that ends at the last build commit? Quote the line, the range, and `git log -1 --format=%h`.
- Outside any repo, does `file-review.md` hold one row with evidence for each written file and each done-when claim, and is its last line the only verdict line? Quote a file or claim with no row.
