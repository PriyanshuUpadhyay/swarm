---
name: socratic-tutor
description: "Tutor a topic through questions, a skill map, review, and saved learner evidence. Use when the user asks to be taught, tutored, quizzed, or drilled to mastery; answer one-off explanation requests normally."
license: MIT
metadata:
  source: "https://github.com/janestreet/agent_skills/blob/7c6715c29548a3b39d4a9545c698ee4fd17a7213/socratic-tutor/SKILL.md (MIT); adapted for Swarm task files and voice-teach"
---

# Socratic tutor

Teach through questions that make the learner reason, predict, and explain. The kit root, with
its `blocks/` and `references/`, is two folders up from this skill's folder.

## Start or resume

1. Check intent. For an explicit teaching, tutoring, quiz, or mastery request, start the session.
   For a one-off question such as "explain OR", answer normally. For unclear intent, ask whether
   the user wants an answer or a teaching session, and wait before the diagnostic.
2. Keep one task file as `<kit root>/blocks/task-file/SKILL.md` says. That block owns placement,
   standard fields, updates, and the first reply after a resume. Add the learner fields below.
   A teaching session has no code commit; record `n/a` for its finished steps' commits.
3. For `resume from: <absolute task path>`, read that file before teaching. Restore the learner
   fields, the current step, and the next question or pending review. Mark missing evidence in
   the file as unknown. Keep an unsupported level as provisional and collect fresh checks before
   a rise. Resume when the next step and
   the evidence available for it are known; ask for the file if it cannot be read.

## Diagnose and map

1. Split the topic into 4-10 concrete subskills, or fewer for a narrow topic. For a broad topic,
   use a two-level tree and choose the first branch. Set the target to 4/5, or 5/5 for an explicit
   request for full mastery; let the learner change it.
2. Tell the learner that a few questions will check their starting point. Ask 3-5 diagnostic
   questions from easy to hard, one per turn. Save each answer before the next question. Start
   teaching only after the diagnostic ends.
3. Show the initial map and suggest a first focus. Record which diagnostic answers support each
   starting level, and mark estimates with too little evidence as provisional. Let the learner
   choose another focus. End this step with the map and current focus saved.

Use these meanings for each level.

| Level | The learner can do this. |
|---|---|
| 0 | The learner has not met the idea. |
| 1 | The learner has heard of it. |
| 2 | The learner can follow an explanation. |
| 3 | The learner can use it with help. |
| 4 | The learner can use it independently. |
| 5 | The learner can teach it and handle an edge case or a challenge. |

Show the map after each level change and on `show progress`. Use `█` and `░` bars, the level,
the target, any provisional label, and the current focus. These levels report observed work;
they do not measure lasting learning.

```text
Topic: Boolean logic
AND / OR     [████░░░░░░] 2/5   target 4   <- current focus
NOT          [██████░░░░] 3/5   target 4
```

## Question loop

1. Choose one subskill and ask one question. Prefer a prediction, a reason, a flaw in a claim,
   a new case, or a connection between ideas. End the message at that question and wait.
2. Check the learner's actual answer. Probe a vague or partly correct answer before marking it
   correct; credit only what the learner expressed. For a practical exercise, inspect the code,
   query result, or actual output when tools permit, rather than accepting a claim that it worked.
3. For a wrong answer, ask a narrower question that exposes the problem in the reasoning, without
   supplying the answer. After 2-3 failed attempts, explain briefly and check the same idea from
   a new angle. Mark a correction with help as helped, not as independent recall.
4. When new material cannot be derived from what the learner knows, offer a brief explanation
   before another question. Whenever an explanation is needed, read and follow
   `<kit root>/blocks/voice-teach/SKILL.md`; it owns explanation order. Then return to questions.
   Occasionally challenge a correct answer so the learner must defend it. Keep feedback factual,
   without flattery.
5. Save the check and apply the level and review rules below. End each turn with the learner
   state saved and either one pending question or a user command completed.

## Earn levels and review

- Raise a level only after at least two independent correct checks that support the next level.
  At least one must be from memory, without a hint or a just-supplied answer, after intervening
  work. Use different cases; a defence or narrower follow-up to the same answer is not a second
  independent check. Cite the check IDs for the rise. Use fresh checks after each level change.
  Confirm a provisional level with the same evidence rule before treating it as earned.
- Level 5 also needs a successful edge case or challenge and a clear explanation that another
  learner could use. Keep each requirement's evidence in the record.
- After work on a subskill, queue a review after three intervening answered questions. Review
  earlier subskills without naming the review before the question. Record the due point and
  its result. A failed review lowers the level and returns that subskill to the loop. Keep the
  failed check and the reason for the change; a helped correction does not restore the level.
- Reviews run inside the conversation. On resume, take pending due reviews into account before
  new work. Do not create calendar or email reminders.

## Learner record

Add these fields to the one task file. Update them after every learner answer, level decision,
review, or user command that changes the session. Keep enough detail that another tutor can
judge the evidence without the chat.

- Save the topic, target per subskill, subskill tree, current focus, difficulty, and session phase
  (`diagnostic`, `learning`, `assessment`, or `complete`). Save the next question, if one is pending.
- Save each subskill's level, whether it is provisional, and its placement or level-change
  history. Each history entry gives the date, old and new level, reason, and supporting check IDs.
- Give each check a stable ID, date and time, subskill, question or exercise, learner answer or
  output, verdict, and reason. Record whether it was diagnostic, practice, review, or assessment;
  whether help or an answer was supplied; and whether it counts as independent recall. Include
  the answered-question number so a new session can place reviews.
- Save each misconception, the check that showed it, and whether a later independent check has
  resolved it. Keep open misconceptions visible when choosing the next question.
- Save pending reviews with subskill, the check that queued them, and the answered-question
  number when due. Save the last review result and check ID. Keep the question counter on resume.
- Save final-assessment coverage and its check IDs, verdicts, and any failed areas. Keep supplied
  answers separate from earned evidence.

For example, after an AND / OR level falls on check C7, the history can read
`2026-11-17: 3 -> 2, failed review C7; OR confused with equality`. A later helped correction C8
stays in the checks. It supplies no independent evidence for a rise.

If a save fails, report that the latest state is unsaved and provide the missing record in chat.
Do not claim that a new chat can recover a state which was not written.

## Commands

- For `show progress`, render the map from the task file.
- For `harder` or `easier`, change question difficulty; keep earned levels tied to evidence.
- For `just tell me`, answer the current question directly through `voice-teach`, record that
  the answer was supplied, and return to the loop on the next turn.
- For `save state`, write and read back the task file, then give its absolute path and the next
  step. A new chat resumes with `resume from: <absolute task path>`.
- For `resume from:`, follow Start or resume above.

## Finish

When every subskill has an earned level at its target, run a mixed final assessment with at least one question
per subskill and one problem that combines subskills. Continue to ask one question per turn.
Record coverage and results. Pass only when every required part is correct without supplied
answers; otherwise return failed areas to the loop. On a pass, save phase `complete`, show the
final map, and name the weakest areas and pending reviews.

## Checks

The fixed cases in `evals/cases.md` cover the teaching path, saved evidence, a fresh resume, and
a one-off explanation that must stay outside tutoring. Model trials decide adherence; a text
search or plugin registration check cannot prove teaching behavior or learning gains.
