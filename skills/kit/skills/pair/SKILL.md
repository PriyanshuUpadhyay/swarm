---
name: pair
description: Coding partner that writes the code with the user. Use when the user wants the agent to write code one behavior at a time, each as one diff of about 40 lines that the user approves, changes, or denies, with a short plan and one commit per finished behavior. Not for whole-task delivery, which `deliver` owns, and not for one pull request, which `review-walk` maps and judges.
---

# Pair

Write the code together, one small chunk at a time. The user approves every chunk before it touches
a file, and every finished behavior is committed at once. The user keeps the mental model; the agent
does the typing.

## One task file per task

Keep one task file, in the run folder that `~/.claude/references/run-folder.md` names, at:

```text
<repo-root>/tmp/pair/<YYYY-MM-DD>-<slug>.md
```

It holds:

- the goal in one line;
- `Repository: <absolute path>` of the product repository, on its own line;
- `Status: active`, on its own line, flipped to `Status: done` after the last behavior. The
  edit gate hook reads `Repository:` and `Status:` and denies Edit and Write inside that
  repository while the status is active, so a chunk can land only through `patch`;
- the ordered behaviors, each one testable sentence;
- the current behavior;
- a chunk log with the chunk number, its intent, its `file:line`, and its outcome
  (`approved`, `changed`, or `denied`);
- the commit hash of each finished behavior;
- one evidence line per finished behavior: the check that ran and its result, or the run that
  showed the behavior on the real surface;
- the checks to run;
- the next step.

Give other agents this absolute path, never a paraphrase or a summary. Read the file first after a
`/clear`, a `/compact`, or a resume, and make the first reply a status block: the behaviors done
with their hashes, the behaviors pending, the current chunk, and the open decisions, then the next
chunk. Nothing done is redone. The file is the truth, the chat is not.

The file is a log for a resume, not a display. Keep it to the fields above, with no source survey
and no essay. Everything the user has to read, which is the behavior list, every chunk, and every
example, appears inline in the chat as text and fenced code. Never point the user at the task file,
and never answer with its path in place of the code.

## Plan first, short

Read the code the task touches. Write 3-7 behaviors, each one testable sentence in dependency
order. Show the list inline in the chat and wait for the user's approval or edits; the copy in the
task file is a record, not the reply. No design essay, no option survey.

When no `flow` folder is named, first run `python3 <flow-skill-dir>/scripts/flow_run.py start`
and follow the `flow` skill's entry rule before the plan. With a
`flow` folder, take the behaviors from the done-when of its `01-frame.md`
and the contracts in its `03-contracts.md`, and hold its build step. Before the first chunk, apply
the rules and the todo list in its `05-build.md`. The flow's review step follows the
last behavior, so pair itself still has no closing pass.

Every behavior ends with `Standards: <row ids>` or `Standards: none`. Select them using
`~/.claude/skills/engineering-standards/SKILL.md`. The agent checks this selection against the
behavior and repository; the user does not have to catch a wrong `none` or write the standards.

## Chunk loop

For the current behavior, propose one chunk: the whole behavior as one diff, about 40 changed
lines, several hunks and files allowed when the behavior needs them. Do not walk a behavior up in
steps of 10, 20, then 40 lines; go straight to one chunk. A behavior that needs much more than 40
lines is two behaviors, so split the list, not the chunk. Show three things and then stop:

1. one line of intent;
2. the exact target `file:line`;
3. the change as a unified diff in a `diff` fence, made by a CLI tool, never a prose
   description of the change.

To make the diff, `cp` the target file into the scratchpad, make the one edit on that copy, and
run `diff -u <target> <copy>`. Never write the whole proposed file out by hand; the copy plus one
edit types only the changed lines. For a new file, write it in the scratchpad and diff against
`/dev/null`. Paste the output as is; the `-` lines show what goes away and the `+` lines show
what comes in. Keep the diff to the hunks of the chunk.

A chunk touches only what its behavior sentence names. No new helper, field, export, flag, or
todo goes in unless the sentence names it. A new symbol lands with its first caller, as the
`sequence-verifiable-units` skill says. An edited function stays where it is, so the diff
shows an edit, not a delete plus an add; a move is its own chunk.

Code stays inline by default. Before a chunk adds or splits out a function, file, or module, apply
`~/.claude/skills/minimize-reader-load/SKILL.md`, and name the condition that holds in the
intent line.

A file delete, a rename, or a merge-conflict resolution is its own chunk and never rides along
with an edit. Its diff is `git diff --stat` for that change plus the file list, and for a
conflict the resolved hunk from `diff -u`. It waits for `approve` like any other chunk.

Then wait for one of three replies:

| Reply | Result |
|---|---|
| `approve` | apply the chunk and run the smallest check for the touched unit (compile, lint, or one test). During the check, prepare the next chunk's diff in scratch from the patched file. After the check passes, show its result in one line and the next chunk's diff in the same reply. When the chunk is the last one of its behavior, also commit the behavior and add the short hash to that line. If the check fails, show the failure, drop the prepared chunk, skip any commit, and wait for `change` or `deny` |
| `change: <text>` | revise the same chunk and show it again |
| `deny` | drop the chunk without argument, ask at most one question, then propose the next chunk |

Example. The intent is "reject an empty phone number in the parser". The target is
`src/parse.py:88`. The reply shows a `diff` fence with 2 `-` lines and 12 `+` lines from
`diff -u`. The user types `approve`. The chunk is applied, `pytest tests/test_parse.py -q`
runs, and the reply is one line: `1 passed`. When the last chunk of the behavior passes, the
behavior is committed as `parse: reject empty phone number` and the reply is `1 passed, a1b2c3d`.

Save the `diff -u` output to `<scratchpad>/chunk-<N>.diff`. On `approve`, apply it yourself with
`patch <target> <scratchpad>/chunk-<N>.diff`. Never spawn a chunk writer or any other worker; the
diff already is the change, and a worker would only re-type it.

Never apply a chunk before `approve`. Never bundle two chunks into one proposal. Exactly one chunk
stands in front of the user at a time, however many agents or panes are working; the others wait in
the task file.

## Commit per behavior

The `approve` of a behavior's last chunk is the commit order. Earlier chunks of the behavior stay
uncommitted in the working tree, so one behavior is one commit. The agent writes the message: a subject of one line in the
imperative, under 60 characters, that names the file or unit and the change; a body is optional
and goes in only when the chunk cut a corner or rejected an option, in two sentences at most.
Never ask "yes to commit" and never ask the user for a commit message. The `decisions` skill still
confirms its buffered decisions before the commit.

## Behavior done

When every chunk of the current behavior is applied and its check passes, show, in this order:

1. one normal example, with its input and its result;
2. one failure example;
3. the test command and its result.

Then propose the first chunk of the next behavior. No commit question; the behavior commits
are the record. After the last behavior, set `Status: done` in the task file, print the flow's `Next:` line for
the review step, and stop. There is no closing pass:
no cleanup gate, no readability review, no extra tooling to set up or tear down.

## Guards

- No edit before `approve`.
- One chunk is one behavior as one diff, about 40 changed lines, several hunks allowed. A bigger
  behavior is split in the list, never walked up in stages.
- A chunk touches only what its behavior names. A new symbol lands with its first caller. An
  edited function stays in place.
- Code stays inline unless `~/.claude/skills/minimize-reader-load/SKILL.md` allows the
  extraction.
- A delete, rename, or conflict resolution is its own chunk, shown as `git diff --stat` plus
  the file list.
- `approve` applies and checks the chunk in one turn, and commits when the behavior is complete.
  No commit question.
- The agent applies its own chunk with `patch`. No chunk writer, no worker, under any host.
- The task ends at the last behavior. No cleanup gate, no readability review, no teardown step.
- The commit subject is one imperative line; a body is optional and short.
- One chunk in front of the user at a time.
- No second plan file. The task file is the only plan.
- No task file that git tracks. `tmp/` stays in `.git/info/exclude`.
- Update the task file after every approved chunk.
- Keep replies to the chunk itself. No progress essays, no restated plan.
- Show the behavior list and every chunk inline in the chat. A file path is never the reply.
- Every chunk is a unified diff from `diff -u`, never a prose description of the change.
