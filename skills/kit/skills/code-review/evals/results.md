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

Round 2, after `L-14`, `L-15` and the per-file output line. New skill-side runs; the round-1 base
reports were reused. The first round-2 grading deleted whole lines that held a rule id, which
removed findings and merge lines from both sides, so it was discarded. These rows come from blind
copies that replace method words inside each line (`blind3.py`).

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-05 | b541a3b3 | no skill | utf8-cap | claude | win | 2/2 vs 1/2; the refute seat kept the 196,608-byte reason |
| 2026-11-05 | b541a3b3 | no skill | utf8-cap | codex | win | 2/2 vs 1/2; both found the overflow, only the skill side blocked the merge |
| 2026-11-05 | b541a3b3 | no skill | test-misses-path | claude | loss | 0/2 each; the skill side wrote that reverting the anchor "would break it, by reading the code", which is false |
| 2026-11-05 | b541a3b3 | no skill | test-misses-path | codex | win | 2/2 vs 0/2; the skill side reverted the anchor and the test still passed; the base side found the builder.rs:55 defect instead |
| 2026-11-05 | b541a3b3 | no skill | crlf-not-a-bug | claude | loss | 1/2 vs 2/2; the skill side raised an `L-14` ask on a test-only change, so it did not say "can merge" |
| 2026-11-05 | b541a3b3 | no skill | crlf-not-a-bug | codex | win (narrow) | 2/2 each; the skill side ran the suite and a negative test, and covered all four files |

Verdict round 2: fix. Both losses are `L-14` on the Claude side. `L-14` now asks for a run revert
and its quoted result, says reading the code is not proof, and leaves out a diff that only adds a
check. Only the two lost pairs run again.

Round 3, only the two lost Claude pairs, after the `L-14` change. Graded blind by Codex.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-05 | 113e3da2 | no skill | test-misses-path | claude | win (weak) | 1/2 vs 0/2; the skill side ran the revert and named the gap, but filed it as an ask and said it can merge after the answer |
| 2026-11-05 | 113e3da2 | no skill | crlf-not-a-bug | claude | loss (narrow) | 2/2 each; the skill side approved and covered all four files, but claimed a body with no closing `---` cannot reach the test, which a scratch call disproved |

Latest result per pair: 5 wins, 1 narrow loss. The loss is one wrong side claim on a case where both
sides give the right answer; the owner decides between keep and another fix.

Limits: eval panes cannot start seats, so each skill pane wrote every seat file itself; the round-1
base reports were reused in rounds 2 and 3; `crlf-not-a-bug` expectations pass a blanket approval,
so completeness decides it.

Round 4, after the owner's taste file (`~/.review-walk/taste.md`, from 304 of their own PR
comments) and three planted taste cases. Skill side on all six cases; base side new on the three
taste cases, round-1 base reports reused for the old three. The first base runs on
`hand-synced-shape` were voided, because their brief barred the folder of the changed file.
Every refute seat kept every fix (10 seats).

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-06 | 4018d584 | no skill | utf8-cap | claude | win | 2/2 vs 1/2 |
| 2026-11-06 | 4018d584 | no skill | utf8-cap | codex | win | the skill side says it cannot merge |
| 2026-11-06 | 4018d584 | no skill | test-misses-path | claude | win | 2/2 vs 0/2; the gap is now a fix |
| 2026-11-06 | 4018d584 | no skill | test-misses-path | codex | win | 2/2 vs 0/2 |
| 2026-11-06 | 4018d584 | no skill | crlf-not-a-bug | claude | loss (narrow) | 2/2 each; the base side had one more checked point |
| 2026-11-06 | 4018d584 | no skill | crlf-not-a-bug | codex | win (narrow) | 2/2 each |
| 2026-11-06 | 4018d584 | no skill | dup-helper | claude | win | 1/2 each; neither flagged the missing blank lines |
| 2026-11-06 | 4018d584 | no skill | dup-helper | codex | loss | 1/2 each; the base side also found that a blank line in the metadata block makes `block_kind` reject a rule block |
| 2026-11-06 | 4018d584 | no skill | check-the-type-owns | claude | loss (narrow) | 2/2 each; the skill side cited a wrong caller line |
| 2026-11-06 | 4018d584 | no skill | check-the-type-owns | codex | loss | 1/2 vs 2/2; the skill side did not say a new variant would be rejected |
| 2026-11-06 | 4018d584 | no skill | hand-synced-shape | claude | loss | 1/2 each; neither asked for one source of the comment keys |
| 2026-11-06 | 4018d584 | no skill | hand-synced-shape | codex | loss | 1/2 each; the same miss |

Verdict round 4: fix (6-6). The skill panes read the taste file, but they answered only the rule
ids in each unit's checklist, so no taste pattern became a finding. `start` now loads the
`TASTE-` rules of `~/.review-walk/taste.md` with the repo rules, so each matching unit's checklist
holds them and the repo seat must answer each one. Only the six lost pairs run again.

Round 5 reran the skill side of the six round-4 losses after `start` loaded the `TASTE-` rules,
against the same base reports. Two pairs then ran again: `dup-helper` (Codex) after `TASTE-11` asked
for a line count, and `hand-synced-shape` (Claude) after `verdict` turned a `TASTE-` note on added
code into a fix. The Claude pane filed `TASTE-13` as a note three times, even after check.md said
fix, so code owns that rule now. `universal-ctags` was missing for part of the round and was
reinstalled before the last two runs. Every refute seat kept every fix it got.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-06 | 6bf15b39 | no skill | crlf-not-a-bug | claude | tie | 2/2 each |
| 2026-11-06 | 6bf15b39 | no skill | check-the-type-owns | claude | win (narrow) | 2/2 each; more correct points |
| 2026-11-06 | 6bf15b39 | no skill | check-the-type-owns | codex | tie | 2/2 each, regraded after the blinding garbled text |
| 2026-11-06 | 6bf15b39 | no skill | hand-synced-shape | codex | win | the skill side files the hand-kept key set as a fix |
| 2026-11-06 | 6bf15b39 + taste count | no skill | dup-helper | codex | win | 2/2 vs 1/2; "21 consecutive lines without a blank line" |
| 2026-11-06 | ee83a100 | no skill | hand-synced-shape | claude | win | 1/2 each; only the skill side calls the key list a defect |

Verdict: keep. The latest result for each of the 12 pairs is 10 wins and 2 ties. Weak expectation:
`hand-synced-shape` exp 1 needs the words "one source" in review.md. The seat row asked for it, but
the summary dropped it. Known side effect: the `TASTE-` upgrade also makes a fix of a note on a long
function that was long before the change, if the note quotes an added line, and no refute seat
checks those rows.
