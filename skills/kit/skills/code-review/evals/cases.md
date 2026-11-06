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

The next three cases test the owner's review taste (redundancy, readability and whitespace, design
that lasts). Each one plants one problem in a real kit file on base `473c4245`. The change in
`files/<case>.patch` applies with `git am`; its tests still pass.

## case: dup-helper
Source: planted; the owner's comments flag a helper declared again when one exists, and a long function body with no blank lines
Task: Review the change 473c4245..4e2fa85 in this repository and write your findings to review.md. Say whether it can merge.
Files: a clone of the repository at 4e2fa85 (`files/dup-helper.patch`)
Check (script): none
Expect:
- review.md says the new `block_kind` in `plan_chart.py` repeats what `meta` in `skills/kit/skills/swarm/scripts/preset.py` already does, and asks to use `meta` or one shared helper.
- review.md asks for blank lines between the steps of `block_kind`, or names its body as one block with no breaks.

## case: check-the-type-owns
Source: planted; the owner's comments flag runtime checks on a value that its type already guarantees
Task: Review the change 473c4245..d7a82a1 in this repository and write your findings to review.md. Say whether it can merge.
Files: a clone of the repository at d7a82a1 (`files/check-the-type-owns.patch`)
Check (script): none
Expect:
- review.md says the new provider-name check in `src/profiles/native.rs` cannot fail, because `Provider` already limits the value to its variants, and asks to remove it or rely on the type.
- review.md says the hard-coded list of ids must be edited by hand for each new `Provider` variant, or that a new variant would be rejected here.

## case: hand-synced-shape
Source: planted; the owner's comments flag a shape that two places must keep in sync by hand
Task: Review the change 473c4245..031c63a in this repository and write your findings to review.md. Say whether it can merge.
Files: a clone of the repository at 031c63a (`files/hand-synced-shape.patch`)
Check (script): none
Expect:
- review.md says the comment shape is now written in two places, `COMMENT_KEYS` in the new status check and the keys that `post` reads, which must change together, and asks for one source.
- review.md says the new quote check does not use `anchor` or `new_side_lines`, or that it ignores the comment's `path`, so a quote from another file passes.
