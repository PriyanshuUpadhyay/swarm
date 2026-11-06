# root-cause-analysis eval results

Name check, 2026-11-06. Claude, GPT, and Gemini, cold, all read `root-cause-analysis` as "find the
underlying cause of a bug or failure".

Each case ran on a clone of swarm at `a0322094` with a planted read-only `data/swarm.db` (the real
schema, synthetic rows). The pane wrote its RCA to `replies.md` and changed no tracked file. The
baseline is no skill. Graders ran on the other provider, blind. Skill task files were moved out
before blinding.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-06 | draft, round 1 | no skill | data-is-there | claude | win | 3/3 vs 2/3; the baseline quoted no query |
| 2026-11-06 | draft, round 1 | no skill | data-is-there | codex | win | 3/3 vs 2/3; the baseline quoted no query |
| 2026-11-06 | draft, round 1 | no skill | code-is-a-lead | claude | loss | 2/3 vs 3/3; the skill side took x-9's stored cost as proof that Codex parsing works |
| 2026-11-06 | draft, round 1 | no skill | code-is-a-lead | codex | loss | 3/3 each; the baseline found that `src/host/usage.rs` writes cost only for Claude |
| 2026-11-06 | draft, round 1 | no skill | what-broke-first | claude | win (narrow) | 1/3 each; both called the valid unacked reports false |
| 2026-11-06 | draft, round 1 | no skill | what-broke-first | codex | win | 3/3 vs 2/3; the baseline kept its queries in notes only |
| 2026-11-06 | round 2, step 6 writer rule | no skill | data-is-there | claude | win | 3/3 vs 2/3 |
| 2026-11-06 | round 2 | no skill | data-is-there | codex | win | 3/3 vs 2/3 |
| 2026-11-06 | round 2 | no skill | code-is-a-lead | claude | win (count only) | 3/3 vs 2/3, but the grader would act on the baseline; the skill side still took x-9's cost as proof |
| 2026-11-06 | round 2 | no skill | code-is-a-lead | codex | win | 3/3 vs 2/3; the skill side found the Claude-only cost writer |
| 2026-11-06 | round 2 | no skill | what-broke-first | claude | loss (narrow) | 2/3 each; the skill side called rows impossible and a chair cost invalid, and its first fix hid the ack signal |
| 2026-11-06 | round 2 | no skill | what-broke-first | codex | win | 3/3 vs 0/3; the baseline opened with a fix and quoted no query |
| 2026-11-06 | round 3, step 6 reworded | no skill | data-is-there | claude | win | 2/3 each on a strict query reading; better code accuracy |
| 2026-11-06 | round 3 | no skill | data-is-there | codex | win | 3/3 vs 2/3; the skill side reported a row with no writer as a question, not as bad data |
| 2026-11-06 | round 3 | no skill | code-is-a-lead | claude | win (narrow) | 3/3 vs 2/3; neither side explained the Codex cost limit |
| 2026-11-06 | round 3 | no skill | code-is-a-lead | codex | loss | 2/3 vs 3/3, only on "cites x-9"; the skill side wrote "this reader never produces Codex dollar cost" |
| 2026-11-06 | round 3 | no skill | what-broke-first | claude | win (narrow) | 2/3 each; the skill side checked duplicate groups and repeat rings |
| 2026-11-06 | round 3 | no skill | what-broke-first | codex | loss (narrow) | 2/3 each; the baseline named the repeated ack problem more clearly; the skill side wins 3/3 to 2/3 if exp 1 reads less strictly |

Verdict: keep (round 3: 4 wins, 2 narrow losses; 13 of 18 over three rounds). Round 1 lost
`code-is-a-lead` on both providers, so step 6 got a rule to find the code that writes each value.
Round 2 overused it on Claude, so the rule was reworded: a value with no writer is a question about
where the data came from, not proof that the data is wrong. The owner chose to stop after round 3.

The planted x-9 cost (0.42 for a Codex agent) has no writer at `a0322094`. It was kept, because
real data from an older version can hold such a row.

Weak: `code-is-a-lead` exp 2 asks for x-9 by name, so it fails a valid other control row, and it
does not ask whether the code can write a Codex cost. `what-broke-first` exp 1 and exp 3 check the
same order. `data-is-there` exp 2 does not say whether abbreviated SQL counts. Graders also asked
the skill to avoid fix options that suppress the reports a real fault raises.
