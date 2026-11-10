---
name: root-cause-analysis
description: "Find why a production issue happened from existing data: one real instance, each claim checked against logs or tables, and what broke named before any fix. Use when asked for an RCA or why something failed for users."
---

# Root cause analysis

Example. Initial state: a support thread says a user got no notes yesterday.
Action: the repo rules list the logs and tables. One query shows that the user's last recording
reached the server with zero audio bytes, and the notes job ran and found nothing to write.
Result: the reply says "the recording sent no audio, so no notes could be made", quotes the query
and its rows, gives the scope (1 user, 1 day), and only then lists fix options.

In real runs the owner corrected four things: an agent asked for a new deploy or new logs when the
tables and logs already held the answer, it called a claim verified from the code alone, it gave a
fix set before it said what was broken, and its explanation had no example.

## Steps

1. **Task file.** Keep one, as `<kit root>/blocks/task-file/SKILL.md` says, at
   `tmp/root-cause-analysis/<YYYY-MM-DD>-<slug>.md`. It holds the symptom, the sources, each claim
   with its status, the queries that ran, and the next step. Update it after each step.
2. **Symptom.** Read the report first, such as the thread, the ticket, or the screenshot. Write the
   symptom in one line, with one real instance: the id, the time, and the exact error or output.
3. **Sources.** List the data that already exists: tables, logs, traces, analytics, and the repo's
   own debug tools. When `~/.root-cause-analysis/rules/<repo>.md` exists, read it, because it names
   the sources of that repo and how to reach them. `<repo>` is the folder name above the git common
   dir. Use these sources before you ask for new logs, a deploy, or a repro. Ask only for access that
   you do not have, and name the query that the access would run.
4. **One instance.** Follow the real instance through each stage of the flow, with the time and the
   record that shows it at each stage. Quote each query and the rows it returned.
5. **What broke.** Write one sentence that says what is broken, with the instance, before any cause
   or fix. When the report has several complaints, give each one a line: shown by the data, not
   shown, or not checked.
6. **Cause.** Rank two or three causes. Give each a prediction that the data can refute, then run
   the query. Mark each claim `checked: <query>` or `from code: <file:line>`. A claim from code
   is a lead, not a finding. For each value that your conclusion rests on, find the code that
   writes it. When the code cannot write that value, say so, and say what it does and does not
   prove. A value with no writer is a question about where the data came from, not proof that
   the data is wrong.
7. **Scope.** Count how many users, events, or runs the cause touches, and in what time range,
   from the data.
8. **Reply.** Give what broke, the cause, the proof (queries and rows), and the scope, with one plain
   example. Put fix options last, as options for the owner. When the cause is a code bug that a
   local command can repeat, continue with `<kit root>/blocks/diagnose/SKILL.md`.

## Rules

- Read production data only. Make no write, deploy, or config change without the owner's approval.
- A finding needs a query and its result. Code, docs, and memory give leads.
- Name what broke before any fix.
- Keep personal data, such as names and emails, out of every PR, issue, and message that you post.
  Redact each secret as `<REDACTED>`.
- The repo rules file is private and stays outside this skill, because it names internal systems.
  Add a source to it when an RCA in that repo used it.
