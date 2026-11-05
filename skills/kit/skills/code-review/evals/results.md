# code-review results

The skill side read `skills/code-review/SKILL.md` and ran check mode alone, writing every seat file
itself; the chair opened the refute seat on the other provider when the first verdict had a `fix`.
The baseline got the same task and "Do not load or read the skill `code-review`". Runs on
code.routine (claude sonnet, codex gpt-6.1-sol), each in its own clone; each pair graded blind by a
checker on the other provider, with lines that named the method removed from `review.md`.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-05 | f0e303a8 | no skill | utf8-cap | claude | loss | 0/2 vs 1/2; both said "can merge"; the skill side bounded the overflow at "2-3 bytes" and missed that each interior invalid byte grows to 3 |
| 2026-11-05 | f0e303a8 | no skill | utf8-cap | codex | tie | 1/2 each; both found the gap and called it an older defect, so both said "can merge" |
| 2026-11-05 | f0e303a8 | no skill | test-misses-path | claude | win (weak) | 1/2 vs 0/2; neither found the test gap; the skill side passed expectation 2 on an unrelated ask, which the grader calls a weak expectation |
| 2026-11-05 | f0e303a8 | no skill | test-misses-path | codex | loss | 1/2 vs 2/2; both found a new long-answer defect at builder.rs:55 (the refute seat kept it); only the base side saw that the live test's 7 events never reach the anchor path |
| 2026-11-05 | f0e303a8 | no skill | crlf-not-a-bug | claude | loss | 2/2 each; the base side covered all four files and the skill side two, with a wrong file count |
| 2026-11-05 | f0e303a8 | no skill | crlf-not-a-bug | codex | win | 2/2 each; the skill side ran the full flow suite and more cases |

Verdict 2026-11-05: fix. Two wins, one tie, three losses. The fix targets:
- `utf8-cap`: the rule "an older defect is a note" let both skill panes approve a change whose own
  comment promised a 64 KiB cap. New lens rule `L-15`: a guarantee the change states is checked past
  the happy path, and a broken one is a `fix`.
- `test-misses-path`: no rule asked whether a new test reaches the changed code. New lens rule
  `L-14`: name the line a test reaches, and show the test fails when the change is reverted.
- `crlf-not-a-bug`: the report listed only findings. Output now gives one line per changed file
  with what was checked.
- Weak expectation: `test-misses-path` expectation 2 passed on an unrelated condition; it now names
  the test.
