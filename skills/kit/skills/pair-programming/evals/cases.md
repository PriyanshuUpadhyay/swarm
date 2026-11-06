# pair-programming eval cases

Each case runs on a clone of swarm at `5822d432` (release 0.0.60), on the branch `main`. The owner
side sends up to two replies after the first answer, and the pane writes every reply it sends into
`replies.md`. The baseline is the last committed pair mode, `presets/implement/references/pair.md`
at `5822d432`.

## case: stale-base
Source: owner, 2026-10-09: "approve but develop was updated. this might conflict later... kepp in
mind"; owner, 2026-11-05: "Already have helpers around this. Can you take a pull of develop so that
you can use those".
Task: pair with me: add a `--json` flag to `skills/kit/presets/implement/scripts/size_gate.py` that
prints the gate result as one JSON object.
Files: the clone's `origin/main` is one commit ahead (`d149a075`, "size_gate: compute the result
once in result(), then print it"). The local `origin/main` ref is set back to the base, so only a
fetch shows the new commit.
Owner replies: (2) "approve. If you asked to rebase or merge, yes, do that first." (3) "approve"
Check (script): none
Expect:
- Before the first diff, replies.md says the branch is behind `origin/main` and names `result()`.
- No diff in replies.md writes the small/raise decision a second time; the JSON output calls
  `result()` or the branch has no rebase and the reply says why.

## case: one-chunk-then-stop
Source: the old pair mode's own guards ("No edit before approve", "Every chunk is a unified diff
from diff -u"), seen kept in the owner's runs of 2026-10-09 to 2026-11-05.
Task: pair with me: `size_gate.py` should not count `*.lock` files toward the line limit.
Files: the clone at `5822d432`; `origin/main` is up to date.
Owner replies: (2) "approve" (3) "approve"
Check (script): after the first reply, `git status --porcelain -- . ':!tmp'` in the clone prints
nothing.
Expect:
- The first chunk in replies.md is one `diff` fence with `---`/`+++` headers and `@@` hunks.
- After the owner's `approve` of the last chunk of a behavior, replies.md shows a check result and
  a short commit hash, and `git log` shows that commit with a one-line subject under 60 characters.

## case: only-what-it-names
Source: owner, 2026-11-05: "we don't need the cache key here"; owner, 2026-10-21: "let's not handle
it right now.... it's usless imo".
Task: pair with me: `size_gate.py` should refuse an empty base argument with a clear message.
Files: the clone at `5822d432`; `origin/main` is up to date.
Owner replies: (2) "approve" (3) "approve"
Check (script): none
Expect:
- The diff handles the empty string only. It adds no code for a whitespace-only, very long, or
  other base value; replies.md may ask about one in a question.
- The intent line of the chunk names the empty-string case (`""` or "empty").
