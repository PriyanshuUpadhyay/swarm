---
name: diagnose
description: "Find a hard bug's cause: one command that goes red on it, a smaller case, one cause tested at a time, then the fix. Use when a bug, failure, or slowdown has no known cause."
license: MIT
metadata:
  kind: step
  owns: "the loop that finds the cause of a bug"
  serves: "build, review"
  needs: "a bug report with its symptom"
  gives: "a red command, a minimized case, the cause, and the fix with its regression check"
  conflicts: ""
  users: "implement, planner"
  source: "mattpocock-skills: skills/engineering/diagnosing-bugs/SKILL.md (MIT); council 2026-10-30-skill-consolidation"
---

## Rules
- Build the loop first. The loop is one command that runs the real code path and goes red on the user's exact symptom. Run it once and keep its output. Until that command exists, do not guess at a cause.
- Make the loop tight: fast, with the same result on each run. Pin the clock, the random seed, and the network. For a bug that comes only sometimes, run the trigger many times until the rate is high enough to work with.
- If you cannot build a loop, stop. Say what you tried, and ask for access to the failing place, a captured artifact, or approval for temporary logs.
- Shrink the case. Remove inputs, callers, config, and steps one at a time, and run the loop after each cut. Stop when each part that stays is needed for the red result.
- Rank a few causes, each with a prediction that can fail ("if X is the cause, changing Y makes the bug go away"). Test one variable at a time, with a debugger or with logs at the boundaries that separate the causes.
- Tag each debug log with one prefix, such as `[DEBUG-a4f2]`, and remove every tagged log before done.
- For a slowdown, measure a baseline first, then bisect.
- Write the regression check before the fix, at a place that runs the real bug pattern, by `blocks/prove-it-works/SKILL.md`. If no such place exists, say so as a finding.
- After the fix, run the loop on the original case, not only on the small one. Name the cause that was right in the commit or PR message.
- Redact each secret as `<REDACTED>` in commands and output that you show.

## Example
Initial state: users report that saving a draft sometimes loses the title.
Action: a script saves 200 drafts against the dev server and counts empty titles: `12 of 200 empty`. Removing the autosave timer keeps it red; removing the second tab turns it green. The cause "two tabs race on the same row" predicts that a version check stops it. A test with two writers fails, the fix adds the check, and the test passes.
Result: the script prints `0 of 200 empty` on the original case, the commit names the race, and `rg 'DEBUG-' src` prints nothing.

## Check (script)
- none: the debug tag and the red command differ for each bug, and the Rules hold the example tag, so the judge reads the work.

## Check (judge)
- Is there one command that went red on the user's symptom before any cause was tested? Quote the command and its red output.
- For the fix, quote the original case red, the minimized case, and the original case green after.
- Is every debug log gone? Quote the search for the tag and its empty result.
