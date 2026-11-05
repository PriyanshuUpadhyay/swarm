# code-review cases

Each case is a commit range of this repository that an earlier review checked, with the finding
that the owner kept or overruled. Each pane gets a clone of the repository at the case's head, so
both sides see the same code. An eval pane may not start seats, so the skill side fills every seat
file itself; the chair then opens the refute seat on the other provider, as
`references/check.md` says.

## case: utf8-cap
Source: tmp/review-check/range-2ffbb0c-7f6f6d0-01/03-verdict.md (fix rows `src/guard.rs:205` and `:209`, fixed in the next range 7f6f6d0..5958a2a)
Task: Review the change 2ffbb0c..7f6f6d0 in this repository and write your findings to review.md. Say whether it can merge.
Files: a clone of the repository at 7f6f6d0
Check (script): none
Expect:
- review.md says that stderr with invalid UTF-8 bytes inside the text (not only at the cut) can make the deny reason longer than 64 KiB, because each invalid byte becomes U+FFFD.
- review.md says the change should not merge as it is.

## case: test-misses-path
Source: tmp/review-check/range-155dbad-5a67c2d-01/03-verdict.md in the permisison-issues worktree (fix row `src/serve/live_rows.rs:292`)
Task: Review the change 155dbad..5a67c2d in this repository and write your findings to review.md. Say whether it can merge.
Files: a clone of the repository at 5a67c2d
Check (script): none
Expect:
- review.md says that the new live test in `src/serve/live_rows/tests.rs` does not reach the code path that the `live_rows.rs` anchor change affects, so the test passes with or without that change.
- review.md says the live test must change before the change merges.

## case: crlf-not-a-bug
Source: tmp/review-check/range-3c01d18-2202309-01/answers.md line 1 (owner: "not a defect: Path.read_text opens in universal-newline mode")
Task: Review the change 3c01d18..2202309 in this repository and write your findings to review.md. Say whether it can merge.
Files: a clone of the repository at 2202309
Check (script): none
Expect:
- review.md does not present a CRLF or Windows line-ending problem in `test_flow_run.py` as a defect that blocks the merge.
- review.md says the change can merge.
