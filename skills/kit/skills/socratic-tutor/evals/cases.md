# Socratic tutor cases

Keep these inputs fixed for the chair's model trials. A new skill's baseline has no tutor skill.
Run both sides with the same model and host. Record the model, skill commit, both outputs, and limits.
The build worker cannot run other agents; these cases have not run.

## case: teach-and-resume
Source: owner, 2026-11-17, option D; r79-tutor.md and its trial T20-T23 found missing check evidence in saved state.
Task: Teach me Boolean logic. I have seen AND and OR in code. Help me reach independent use.
Files: none at start; carry only the saved task file into a fresh tutor session for the resume part.
Check (script): none; check the transcript and the saved file against the expectations.
Expect:
- The tutor asks 3-5 diagnostic questions, one per turn, before teaching, then shows the subskill map.
- After a wrong answer, the next question is narrower and does not supply the answer. A helped correction does not raise a level.
- Each level rise cites two distinct correct checks since the previous level change, with at least one from memory after intervening work.
- A failed review lowers the level and puts the subskill back into the learning loop. The learner script answers the first review wrong, so this occurs within the cap.
- After `save state`, the task file holds check questions, learner answers, help, dates, level evidence, misconceptions, and pending reviews.
- In a fresh session, `resume from: <absolute task path>` restores the current step and the pending review without inventing missing evidence or repeating finished diagnostics.
- The course stays incomplete until targets and the mixed final assessment pass.

## case: explain-once
Source: janestreet/agent_skills 7c6715c29548a3b39d4a9545c698ee4fd17a7213, intent guard; r79-tutor.md says a one-off explanation must stay outside tutoring.
Task: What does OR mean in Boolean logic? Give me one example.
Files: none.
Check (script): none; check the reply and working folder.
Expect:
- The reply defines OR as true when at least one input is true (true when both are), gives exactly one example, and starts no diagnostic, learner map, or tutoring task file.
