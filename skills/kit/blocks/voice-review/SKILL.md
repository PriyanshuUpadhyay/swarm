---
name: voice-review
description: "Rewrite review findings as short PR comments in the reviewer's own voice, from their voice file. Use when review findings become comments that a person will post."
license: MIT
metadata:
  kind: rule
  owns: "the private voice file path and how review comments sound"
  serves: "review, close"
  needs: "review findings with file and line"
  gives: "comments in the reviewer's voice"
  conflicts: ""
  users: "plan"
  source: "skills/review-walk/SKILL.md, step 4 (Comments) and Voice"
---

## Rules
- Read `~/.review-walk/voice.md` and follow it. It holds how the reviewer writes, from their own past reviews, and it stays private, so never copy it into a repo.
- With no voice file, write short plain comments: one clause, a question when the fix is a guess, no praise, no severity labels, and "potential issue, not confirmed:" before an unproved defect.
- Use a file-level comment for a note about the whole file, a repeated pattern, or a reminder. Use a line-level comment for a note about one expression or call.
- Keep each finding's meaning. Change only the words.
- Drop a finding that the reviewer already said in an earlier round or in a pending comment.

## Example
Initial state: a finding says "`fetchAll` runs one query per user in a loop, which may time out for large teams; the same pattern is in `sync.ts`."
Action: rewrite it in the voice file's style.
Result: "lookup in a loop, might time out. batch it. similar patterns too", as a line-level comment on the loop.

## Check (script)
- none: the comments live in the run's output, and only a judge can compare them with the voice file.

## Check (judge)
- Does each comment follow the voice file's length, case, and opener rules, and keep its finding's meaning? Quote the comment, the finding, and the voice rule.
