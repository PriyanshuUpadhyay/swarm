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

## Skill draft, 2026-11-05

The skill side read `skills/research/SKILL.md` (draft `2b63e0f3`, then the step 7 fix) and did the
web part itself. The baseline got the same task and the line "Do not load or read the skill
`research`". Runs on code.routine (claude sonnet, codex gpt-6.1-sol); each pair graded blind by a
checker on the other provider. Cases `recent-compliance` and `origin-number` are new.

| date | skill version | baseline | case | provider | grader | result | grader note |
|---|---|---|---|---|---|---|---|
| 2026-11-05 | 2b63e0f3 | no skill | recent-compliance | claude | checker codex | weak win | 2/4 each; skill gave two dated compliance studies (2611.04367, 2610.30571), base one; the grader says the winner flips if "two findings" may come from one paper |
| 2026-11-05 | 2b63e0f3 | no skill | recent-compliance | codex | checker claude | win | skill 4/4, base 3/4: base gave "seven runs with a rule" where the audit says five were controls |
| 2026-11-05 | 2b63e0f3 | no skill | recent-compliance | codex (rerun) | checker claude | loss | 3/4 each; skill cited an X blog page that answers 403 to a plain fetch, so its link script failed; the grader calls the skill side stronger on substance |
| 2026-11-05 | step 7 fix | same base report | recent-compliance | codex | checker claude | win (close) | 3/4 each, both scripts exit 0; skill gave three compliance findings, base one; skill gave a wrong update date |
| 2026-11-05 | 2b63e0f3 | no skill | origin-number | claude | checker codex | win | 3/4 each; both found v1 "4%" and v3 "2.4%, p = 0.21"; the grader preferred the skill side's precision |
| 2026-11-05 | 2b63e0f3 | no skill | origin-number | codex | checker claude | win | 4/4 each; skill led with the correction and showed v2 already said 2.4%; base opened with "Yes." |
| 2026-11-05 | 2b63e0f3 | no skill | origin-number | codex (rerun) | checker claude | win | 4/4 each; skill linked and dated all three versions |

Verdict 2026-11-05: keep. Six wins and one loss; the loss had a cause the skill could fix (step 7:
a link a reader cannot open without the browser gets an archive copy or is dropped), and that pair
won after the fix. The notes rule for `blocked:` lines came after the last run and is not tested.

Limits:
- The Codex panes could not run `reach.py`: the code.routine sandbox proxy refused shell network
  calls, also with `sandbox_workspace_write.network_access=true`. Their wins come from the skill's
  steps, not from `reach`. Only the Claude pair used `reach`.
- The Codex checkers' link scripts failed the same way (`Tunnel connection failed: 403`); those
  graders opened each page with their own web tool.
- Expectation 1 of `recent-compliance` did not say whether two findings need two papers, and
  expectation 2 had no rule for a new revision of an old paper. Both were made exact after these
  runs.
