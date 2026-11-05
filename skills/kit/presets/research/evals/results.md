# research results

Solo mode: the skill side read `PRESET.md` and its blocks and did the web step itself, with no
seats. The baseline got the same task and read no kit file. Runs on code.routine (claude sonnet,
codex gpt-6.1-sol); each pair graded blind by a checker on the other provider.

| date | skill commit | baseline | case | provider | grader | result | grader note |
|---|---|---|---|---|---|---|---|
| 2026-11-05 | 8d617a9e | no skill | agents-md | claude | checker codex | loss | skill 3/4, base 4/4: cited an unnamed "secondary summary" for a significance claim that the paper's Table 1 contradicts |
| 2026-11-05 | 8d617a9e | no skill | agents-md | codex | checker claude | loss | both 4/4: skill led with its weakest result (p=0.25, n=4), claimed "Source agreement: High" with no basis, and added search log and checklist text |
| 2026-11-05 | 8d617a9e | no skill | compiled-skills | claude | checker codex | loss | skill 3/4, base 4/4: said "5 trials each" where the paper's appendix gives three |
| 2026-11-05 | 8d617a9e | no skill | compiled-skills | codex | checker claude | loss | both 4/4: skill cited old v1 numbers that v4 replaced, and its extra length was a source map, a checklist, and run internals |

Script: all 8 reports had 0 dead links (3 to 12 links each). All 4 baselines found the key paper.

Verdict 2026-11-05: do not build `research` as a skill. The skill side lost 4 of 4 pairs. The
cases did not need the method, because plain web search found the key paper each time. The
method added length, process text, and claims, and each loss came from one of those added claims.

Limits: solo mode tests the method, not the web seats. Two cases only, both with a known answer
that plain search finds.

## Full mode, 2026-11-05

The skill side was a real research run (plan, local step, three web seats, gate, merge, report) with
the chair on claude opus. The baseline was one chat pane (claude opus) with no kit. One checker
(codex gpt-6.1-sol) graded the pair blind.

| date | skill commit | baseline | case | provider | grader | result | grader note |
|---|---|---|---|---|---|---|---|
| 2026-11-05 | 8d617a9e | no skill | agents-md | claude (full mode) | checker codex | loss | both 3/4; skill 13 links, base 5, both 0 dead; skill had 8 wrong or narrowed claims (odds read as compliance, a sample count not on the page, the 471.5K-token total given to one file, wrong issue dates), base had 2 (venue label, a speed cause not tested) |

Verdict 2026-11-05: still do not build `research` as a skill. The seats found five real sources the
baseline missed, and the gate dropped two false seat claims. But the merge copied seat and local
numbers without a second read, so more sources gave more wrong claims. The grader picked the
baseline because its fewer claims were exact.
