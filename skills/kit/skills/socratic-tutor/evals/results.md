# socratic-tutor results

The skill side read a copy of `skills/socratic-tutor/SKILL.md`; the baseline got the same brief and
"Do not load or read the skill `socratic-tutor`". Tutors ran on code.routine (claude sonnet, codex
gpt-6.1-sol). For teach-and-resume the chair relayed each tutor message to a Sonnet learner pane with
a fixed script (OR is false when both inputs are true; NOT (A AND B) = NOT A AND NOT B), sent
`save state` after 14 learner replies or when the tutor ended the session, then opened a fresh tutor
pane with `resume from: <path>` for up to 4 learner replies. Each pair was graded blind by a
checker on the other provider, with the sentences that said the skill was or was not read removed.

| date | skill commit | baseline | case | provider | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-17 | 4be44386 | no skill | teach-and-resume | claude | win | 5/7 vs 0/7; skill side asked 6 diagnostic questions, not 3-5; no review failed within the cap; base taught in 7 turns, called the course complete, saved a narrative notes.md, and resumed with a topic menu |
| 2026-11-17 | 4be44386 | no skill | teach-and-resume | codex | win | 6/7 vs 0/7; no review failed within the cap; base asked one diagnostic, kept no map or levels, said "Complete." after about five learner turns, and resumed with no question |
| 2026-11-17 | 4be44386 | no skill | explain-once | claude | tie | 1/1 each; both answered directly with a valid example |
| 2026-11-17 | 4be44386 | no skill | explain-once | codex | tie | 1/1 each; grader: the expectation did not check correctness or example count |

Verdict 2026-11-17: weak keep (two wins, two ties, no loss; two cases, so add a third).
- Weak expectation: "a failed review lowers the level" cannot pass when no review fails within the
  cap. The learner script now makes the first review answer wrong.
- Weak expectation: explain-once now checks a correct inclusive-OR definition and exactly one example.
- Name check 2026-11-17: Claude (opus) and Codex (gpt-6.1-sol) panes each read `socratic-tutor` as
  teaching through guiding questions; none said unsure. `council.gemini` resolved to gpt-6.1-sol, so
  Gemini was not asked.
- Limits: one run per side and provider. The skill snapshot had no `blocks/`, so the Claude skill
  tutor read `task-file` and `voice-teach` from the main kit. The chair's relay passed one Swarm
  "finished its turn" notice to the Claude base tutor as a learner reply. Shrink (skill-eval step 6)
  has not run.
