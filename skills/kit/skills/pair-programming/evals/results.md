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
