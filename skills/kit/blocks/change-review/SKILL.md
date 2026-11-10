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
  source: "skills/code-review/references/check.md (Contract, Run); docs/council 2026-10-13 block rules (R-f: a plan needs a block that runs the full review)"
---

## Rules
- The kit root is two folders up from this file. Read `<kit root>/skills/code-review/references/check.md` and run it whole on `<base>..<head>`: the commit before the first build step, up to the last build commit. Inside a repo, this block adds no rule, lens, or verdict of its own.
- The session that wrote the change never fills the checklist. The rows come from new seats, as that file says, started by the `run-workers` block.
- Inside a repo, pass on only the verdict line that `review_check.py verdict` printed. `INCOMPLETE` is not a pass.
- On `REQUEST CHANGES`, fix each finding as its own unit by the `checked-units` block, then review the new range again.
- When `git -C <repo-root> rev-parse` finds no work tree, the piece has no commits, so new seats review its files instead. Each seat writes rows `| file or claim | pass, fix, or note | evidence |` to `<run>/file-review.md`, with evidence: a rendered frame, a command output, or a `file:line`. Full rows go to the deliverable and to each done-when claim of the frame. Notes, proof files, frames, and run files get one `consistency` row each, which checks only that they agree with the deliverable.
- `<kit root>/blocks/change-review/scripts/file_review.py` owns the stop. After every seat reports, the chair runs it with `--close`, which writes the one verdict line. With fix rows, fix them, run `--next`, and in the next round review only the files changed since the last round. The cap is 2 rounds. At the cap, `--close` writes `Verdict: BLOCKED (<n> fix)`, and the open rows go to the owner; that is no approval. Only an owner line `continue: file-review` in `<run>/answers.md` allows one more round. Inside a repo, a file review never replaces the commit review, and `review_check.py` owns its loop.

## Example
Initial state: the build steps made the commits `a1b2c3d..HEAD`.
Action: run the check on that range. The verdict prints `REQUEST CHANGES` with `C-7`. Fix it in one commit and run the review again.
Result: the second verdict prints `APPROVE`, and the step quotes that line and its range. For a loop outside any repo, two seats write full `file-review.md` rows for `index.html`, `loop.mp4`, and each claim, and one `consistency` row for `notes.md`; `file_review.py <run> --close` writes `Verdict: APPROVE`.

## Check (script)
- `sed -n 3p <repo-root>/tmp/review-check/<review-run>/03-verdict.md` starts with `Verdict: APPROVE`: the newest review approved the range.
- `git -C <repo-root> rev-parse --is-inside-work-tree >/dev/null 2>&1 || python3 -I <kit root>/blocks/change-review/scripts/file_review.py <run>` exits 0: outside any repo, the file review approved the written files within the round cap, with no open `fix` row.

## Check (judge)
- Inside a repo, does the step quote the verdict line as the script printed it, for a range that ends at the last build commit? Quote the line, the range, and `git log -1 --format=%h`.
- Outside any repo, does `file-review.md` hold a full row with evidence for the deliverable and each done-when claim, and only one `consistency` row for each note, proof, frame, or run file? Quote a claim with no row, or a full row on a file that is not the deliverable.
