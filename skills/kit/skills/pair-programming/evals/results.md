# pair-programming eval results

Name check, 2026-11-06. Claude, GPT, and Gemini, cold, all read `pair-programming` as writing code
with the user step by step. Claude and GPT read `pair` as "unsure", so the long name stays.

Round 1, 2026-11-06. The baseline is the last committed pair mode,
`presets/implement/references/pair.md` at `5822d432`, because the skill replaces it. The owner sent
up to two replies after the first answer. The panes used clones of swarm at `5822d432`; graders ran
on the other provider, blind.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-06 | uncommitted draft | pair.md at 5822d432 | stale-base | claude | win | 2/2 vs 0/2; only the skill side said "1 commit behind `origin/main`" and reused `result()` |
| 2026-11-06 | uncommitted draft | pair.md at 5822d432 | stale-base | codex | win | 2/2 vs 0/2; the baseline wrote a second result dict in `main()` on the old base |
| 2026-11-06 | uncommitted draft | pair.md at 5822d432 | one-chunk-then-stop | claude | win (narrow) | 2/2 each; the skill side committed both behaviors |
| 2026-11-06 | uncommitted draft | pair.md at 5822d432 | one-chunk-then-stop | codex | loss (narrow) | 2/2 each; the baseline put the whole task in one chunk and finished inside the turn limit |
| 2026-11-06 | uncommitted draft | pair.md at 5822d432 | one-chunk-then-stop | codex (rerun) | tie | 2/2 each; the baseline test was a little stronger |
| 2026-11-06 | uncommitted draft | pair.md at 5822d432 | only-what-it-names | claude | win | 2/2 vs 0/2; the baseline also refused a whitespace base and named no case |
| 2026-11-06 | uncommitted draft | pair.md at 5822d432 | only-what-it-names | codex | tie | 2/2 each |

Verdict: keep (4 wins, 2 ties, 0 losses; the split case was rerun once and tied). Weak case:
`one-chunk-then-stop` caps the owner at two replies, so a run that splits a task into small chunks
is cut off before the end, and the grader then rewards the run that bundled it. Both of its
expectations are guards that the old pair mode already had, so it can only tie or lose on
completeness. Shrink was not run.


Round 2026-11-11, prompt audit. The skill side is `f63f8338`, the baseline is `9d924cfd`. The clones
and owner replies are as in round 1; the upstream commit was made again with the same subject.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-11 | f63f8338 | 9d924cfd | stale-base | claude | loss (narrow) | 2/2 each; the baseline's applied chunk also changed the usage line |
| 2026-11-11 | f63f8338 | 9d924cfd | stale-base | codex | loss (narrow) | 2/2 each; the skill side's first chunk only accepted the flag |
| 2026-11-11 | f63f8338 | 9d924cfd | only-what-it-names | claude | tie | 2/2 each |
| 2026-11-11 | f63f8338 | 9d924cfd | only-what-it-names | codex | tie | 2/2 each |

Verdict 2026-11-11: keep, with a watch. Both stale-base losses pass every expectation and differ in
how the behavior list split the flag from its output. No expectation tests that split, so the next
case should.

Rerun 2026-11-11 of stale-base, same baseline. Codex ran on `98498f8e`; Claude ran on `98498f8e`
and then on `3dda4775`, which keeps a behavior's usage text and test inside that behavior.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-11 | 98498f8e | 9d924cfd | stale-base | codex | win (narrow) | 2/2 each; the skill side's diff changed the usage line its behavior list promised |
| 2026-11-11 | 98498f8e | 9d924cfd | stale-base | claude | loss (narrow) | 2/2 each; the usage line again went to a second behavior that was not applied |
| 2026-11-11 | 3dda4775 | 9d924cfd | stale-base | claude | win (narrow) | 2/2 each; one chunk with the usage line and full JSON checks for small and raise |

Verdict after rerun: keep with `3dda4775`. The eval brief keeps the pane from reading other blocks,
so `Standards:` is always `none` and this case does not test that step.

Eval 2026-11-12 of the Align, Verify, review, ship change, on the three cases added in `216c705d`.
The baseline is the skill at `44c68fe1` (before `c45efffa`). Three rounds ran; each round had 12 run
panes and 6 blind checker panes, and each pair's checker ran on the other provider. Rounds 1 and 2
each lost `one-chunk-with-align` on Claude, so the skill was fixed twice: `b02a99f7` (never re-ask
the task text or a recorded choice) and `3f9eb8af` (a small-task Align block and a settled-question
test). Round 1 used an earlier form of the cases (three owner replies, word checks); the checkers
named those checks weak, so rounds 2 and 3 use the cases as committed, with the repository test
script as the script check. In rounds 2 and 3 that script printed `ok` on the last commit of all 24
runs.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-12 | c45efffa | 44c68fe1 | open-choices | claude | win | 3/3 vs 1/3; round 1, earlier case form |
| 2026-11-12 | c45efffa | 44c68fe1 | open-choices | codex | win | 3/3 vs 1/3; round 1; baseline chose `--output` itself |
| 2026-11-12 | c45efffa | 44c68fe1 | verify-then-done | claude | win | 2/3 vs 0/3; round 1; asked what the task text states |
| 2026-11-12 | c45efffa | 44c68fe1 | verify-then-done | codex | win | 3/3 vs 1/3; round 1; baseline wrote "The task is done" |
| 2026-11-12 | c45efffa | 44c68fe1 | one-chunk-with-align | claude | loss | 2/3 vs 3/3; round 1; first diff in reply 3, and the scripted reply approved a list not yet shown |
| 2026-11-12 | c45efffa | 44c68fe1 | one-chunk-with-align | codex | win (narrow) | 3/3 each; round 1 |
| 2026-11-12 | b02a99f7 | 44c68fe1 | open-choices | claude | win | 3/3 vs 1/3; round 2 |
| 2026-11-12 | b02a99f7 | 44c68fe1 | open-choices | codex | win | 3/3 vs 1/3; round 2 |
| 2026-11-12 | b02a99f7 | 44c68fe1 | verify-then-done | claude | win | 2/3 vs 0/3; round 2; asked what the code answers |
| 2026-11-12 | b02a99f7 | 44c68fe1 | verify-then-done | codex | win | 3/3 vs 1/3; round 2 |
| 2026-11-12 | b02a99f7 | 44c68fe1 | one-chunk-with-align | claude | loss | 2/3 vs 3/3; round 2; a question the request settles, and the exclusion ask on a small task |
| 2026-11-12 | b02a99f7 | 44c68fe1 | one-chunk-with-align | codex | tie | 3/3 each; round 2 |
| 2026-11-12 | 3f9eb8af | 44c68fe1 | open-choices | claude | win | 3/3 vs 1/3; round 3 |
| 2026-11-12 | 3f9eb8af | 44c68fe1 | open-choices | codex | win | 3/3 vs 1/3; round 3 |
| 2026-11-12 | 3f9eb8af | 44c68fe1 | verify-then-done | claude | win | 3/3 vs 1/3; round 3 |
| 2026-11-12 | 3f9eb8af | 44c68fe1 | verify-then-done | codex | win | 3/3 vs 1/3; round 3; baseline says done with no review |
| 2026-11-12 | 3f9eb8af | 44c68fe1 | one-chunk-with-align | claude | win | one more expectation passed; the baseline re-asked the `*.lock` glob |
| 2026-11-12 | 3f9eb8af | 44c68fe1 | one-chunk-with-align | codex | tie | 3/3 each; round 3 |

Verdict after round 3: keep with `3f9eb8af` (5 wins, 1 tie, no loss, three cases). Limits: the owner
side was scripted, so the eval does not show whether the questions fit real tasks; a pane cannot
start a review seat, so the review itself was not run; the shrink step was not run. Checker notes
still open: no expectation tests the write-failure path of `open-choices`, and the third expectation
of `verify-then-done` holds two facts.
