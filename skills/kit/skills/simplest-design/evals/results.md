# simplest-design eval results

Name check, 2026-11-06. Claude, GPT, and Gemini, cold, all read `simplest-design` and `yagni-check`
as the job; all read `design-review` as a general review. The owner chose `simplest-design`,
because two of the five source corrections were about parts that repeated a value the code already
had, which is not a future need.

Each case ran on a clone of swarm at `7b1752f3`. The pane proposed a design and wrote no code. The
baseline is no skill. Graders ran on the other provider, blind.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-06 | draft, round 1 | no skill | value-already-there | claude | win | 2/2 vs 1/2; the baseline added a `"via"` field to each JSON line |
| 2026-11-06 | draft, round 1 | no skill | value-already-there | codex | win | 2/2 each; the baseline also edited the research skill |
| 2026-11-06 | draft, round 1 | no skill | fixed-list-no-config | claude | loss | 1/2 each; the skill side subtracted lock lines under `tests/` twice, and used `*.lock` |
| 2026-11-06 | draft, round 1 | no skill | fixed-list-no-config | codex | win (narrow) | 1/2 each; the skill side tested that lock files still count as files |
| 2026-11-06 | draft, round 1 | no skill | unreleased-rename | claude | win | 2/2 vs 1/2 |
| 2026-11-06 | draft, round 1 | no skill | unreleased-rename | codex | win | 2/2 vs 1/2; the baseline gave no reason for "no rewrite" |
| 2026-11-06 | round 2, with step 5 "Walk one input" | no skill | value-already-there | claude | win | 2/2 vs 1/2 |
| 2026-11-06 | round 2 | no skill | value-already-there | codex | win | 2/2 vs 1/2; the baseline added a `--json` flag and a route field |
| 2026-11-06 | round 2 | no skill | fixed-list-no-config | claude | win | 2/3 each; exact lock names, so `notes.lock` still counts |
| 2026-11-06 | round 2 | no skill | fixed-list-no-config | codex | win | 2/3 vs 1/3; the baseline subtracted lock lines from both totals |
| 2026-11-06 | round 2 | no skill | unreleased-rename | claude | win | 2/2 vs 1/2 |
| 2026-11-06 | round 2 | no skill | unreleased-rename | codex | win | 2/2 vs 1/2 |

Verdict: keep (round 2: 6 wins, 0 losses). Round 1 lost one result, so step 5 "Walk one input"
was added and all cases ran again. After round 1, `fixed-list-no-config` got exp 3 (a lock file
under `tests/` is subtracted once) and `unreleased-rename` exp 2 was made exact; the owner approved
both. Weak: `fixed-list-no-config` exp 2 (only `size_gate.py` and its test) fails every side that
updates `references/small.md:18`, so it cannot separate the sides. Graders also asked for
expectations on the exact rename sites and on a near-miss lock name. Shrink was not run.
