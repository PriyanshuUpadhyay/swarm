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

# Cases for Align, Verify, review, ship (2026-11-12)

These three cases run like the cases above: a clone of swarm at `5822d432` on `main`, with
`origin/main` up to date. The owner side now sends up to four replies. The baseline is the pair
skill at the commit before the Align and Verify change.

## case: open-choices
Source: owner, 2026-11-11: "At most three questions will not solve it. There cannot be a hard
limit. There might be unresolved questions that are essential for the plan to work."; council log
`2026-11-11-daily-workflow.md`, step 2.
Task: pair with me: `skills/kit/presets/implement/scripts/size_gate.py` should also save its result
to a file.
Files: the clone at `5822d432`.
Owner replies: (2) "Take your default for each question, except one: the file is written only when
I pass `--out <path>`. My example: on a small change, `size_gate.py <base> --out /tmp/r.txt` leaves
the same `small: ...` line in `/tmp/r.txt` that it prints. Not built: no JSON and no append mode."
(3) "approve" (4) "approve" (5) "approve"
Check (script): `python3 -I skills/kit/presets/implement/scripts/test_size_gate.py` in the clone
prints `ok` on the last commit.
Expect:
- Before any behavior list or diff, replies.md asks the open choices as one numbered list, each
  with a default, and one of them asks where or when the file is written.
- No diff writes a file when `--out` is absent, and no diff adds JSON output or an append mode.
- The task file lists the answers, marks which ones the owner typed and which are defaults, and
  records in any wording that JSON and an append mode are not built.

## case: verify-then-done
Source: `pair-programming/SKILL.md` at `3dda4775`: pair "has no closing pass"; council log, steps 6
to 8; kem_glitch reel of 2026-11-09: "I verify the way a user would".
Task: pair with me: `size_gate.py --version` should print `size_gate 1` and exit 0.
Files: the clone at `5822d432`.
Owner replies: (2) "No open questions from me. My example: in the clone,
`python3 skills/kit/presets/implement/scripts/size_gate.py --version` prints `size_gate 1` and
exits 0. Not built: no other flag." (3) "approve" (4) "approve" (5) "approve"
Check (script): `python3 -I skills/kit/presets/implement/scripts/test_size_gate.py` in the clone
prints `ok` on the last commit.
Expect:
- replies.md asks no question that the task text or the code already answers.
- After the commit, replies.md shows the command of the owner's example, run on the committed
  revision, with its real output and the short hash.
- The last reply names the fresh review as still open and does not say that the task is done, and
  the task file of this task still says that it is active.

## case: one-chunk-with-align
Source: case `one-chunk-then-stop` above, with the reply that the new Align step needs.
Task: pair with me: `size_gate.py` should not count `*.lock` files toward the line limit.
Files: the clone at `5822d432`.
Owner replies: (2) "Yes to all defaults. My example: a change that only touches `Cargo.lock` gives
`small`. Not built: nothing else." (3) "approve" (4) "approve" (5) "approve"
Check (script): after the first reply, `git status --porcelain -- . ':!tmp'` in the clone prints
nothing, and `python3 -I skills/kit/presets/implement/scripts/test_size_gate.py` in the clone
prints `ok` on the last commit.
Expect:
- The first chunk in replies.md is one `diff` fence with `---`/`+++` headers and `@@` hunks.
- After the owner's `approve` of the last chunk of a behavior, replies.md shows a check result and
  a short commit hash.
- Each question in replies.md is a choice that the task text and the code leave open, and the
  first diff comes by reply 3.
