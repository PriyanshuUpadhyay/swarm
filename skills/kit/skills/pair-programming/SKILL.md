---
name: pair-programming
description: Write code with the user one approved diff at a time, committing each finished behavior. Use when the user says pair, pair with me, or let's pair, or when an implement run builds in pair mode.
---

# Pair programming

Example. The user says "pair with me: reject an empty phone number in the parser". The branch is 2
commits behind `origin/develop`, and one of them adds `isBlank()` in `src/text.ts`. The first reply
says so and asks to rebase. After the rebase, the reply asks one align round: "1. A phone of only
spaces: reject too? (default: yes) 2. Max length? (default: none)". The user types `1 yes, 2 yes`.
The user writes the acceptance example and one exclusion. The reply shows one behavior and one
`diff -u` chunk that calls `isBlank()`. The user types `approve`. The chunk is applied, one test
runs, and the behavior is committed as `parse: reject empty phone number`. After the last behavior,
the reply says `Verify: a1b2c3d, parse " " -> error "phone required"`, and asks for one review seat.

Write the code together, one small chunk at a time. The user approves every chunk before it touches
a file, and every finished behavior is committed at once. The user keeps the mental model; the agent
does the typing. The kit root, with its `blocks/` and `references/` folders, is two folders up from
this skill's folder.

## One task file per task

Keep the task file as the kit block `blocks/task-file/SKILL.md` says, at
`<repo-root>/tmp/pair/<YYYY-MM-DD>-<slug>.md`. Besides the block's fields, it holds:

- `Repository: <absolute path>` of the product repository, on its own line;
- `Status: active`, on its own line. It stays active through Verify and review, so a review fix
  also lands only through `patch`. Flip it to `Status: done` when the user says the change shipped,
  or when the review passed and the user says done. The edit gate hook reads `Repository:` and
  `Status:` and denies Edit and Write inside that repository while the status is active, so a chunk
  can land only through `patch`;
- `Base: <upstream ref> <short hash>`, from the base check below;
- the align answers as numbered lines `A1..An`, each marked `typed` or `default`;
- `Acceptance:` and `Not built:`, from the align section below;
- the behaviors as the block's steps, each one testable sentence;
- a chunk log with the chunk number, its intent, its `file:line`, and its outcome
  (`approved`, `changed`, or `denied`).

The status block after a resume also names the current chunk. Apply the loaded agent instructions
to user-facing output. The chunk loop below owns the diff shown for approval.

## Base check first

In two real runs the branch was behind the shared branch: one chunk later conflicted, and another
wrote a helper that the shared branch already had. So before the behavior list:

1. Find the upstream branch: the branch's tracked upstream, else `origin/<default branch>`.
2. Run `git fetch` for it, then `git rev-list --count HEAD..<upstream>`.
3. When the count is 0, write `Base: <upstream> up to date` in the reply. When it is more than 0,
   run `git log --oneline HEAD..<upstream>` and `git diff --stat HEAD...<upstream>`. Name the
   commits and any helper, type, or file in them that the task touches, and ask to rebase or merge
   before chunk 1. The rebase is the user's choice; do not run it before they say so.

When the fetch fails, say so in one line and go on.

## Align in rounds

Read the code the task touches first. Then ask the questions that change what gets built. Each
round is one numbered list of every open direction-changing question whose prerequisites are
settled, each with a default that `yes` accepts. There is no limit on rounds or questions.

- Never ask what the code or the task already answers; look it up.
- Each number, size, or limit that the agent invents is a question, with the number as its default.
- A task with no open choice gets no question.
- Ask one question alone when its answer controls the next ones, or when the user asks for single
  questions.
- Align ends when no open question can change the behavior list, or when the user says `enough`.
  After `enough`, the open choices stay listed in the task file, and they block the behaviors that
  depend on them.

Write the answers in the task file as `A1..An`, each marked `typed` (the user wrote it) or
`default` (the user accepted it), and a `Not built:` list.

Before the behavior list, ask the user to write one acceptance example (start state, action,
expected result) and one exclusion, in their own words. The agent never drafts them. When the task
text already holds such an example, quote it and ask only for what is missing. Record them under
`Acceptance:` and `Not built:`. They keep a run of `yes` answers from standing in for the user's
intent.

## Plan first, short

Write 1-7 behaviors, each one testable sentence in dependency order. Each behavior names the
A-numbers it comes from (or `request`) and the command or click that proves it. Show the `default`
answers in one block above the list. A behavior is one thing the user can see work, so its usage text, help line, and test go
with it, not into a later behavior. Show the list inline in the chat and wait for the user's approval or edits; the copy in the
task file is a record, not the reply. No design essay, no option survey.

When an `implement` run calls this skill for its build step, take the behaviors from the done-when
of its `01-frame.md` and the contracts in its `03-contracts.md`, and hold its build step. Before the
first chunk, apply the rules and the todo list in its `05-build.md`. In that case, skip Align, the
acceptance example, and Verify, review, ship, because the run's frame and review steps own them.

Every behavior ends with `Standards: <row ids>` or `Standards: none`. Select them using
the kit's `blocks/engineering-standards/SKILL.md`. The agent checks this selection against the
behavior and repository; the user does not have to catch a wrong `none` or write the standards.

## Chunk loop

For the current behavior, propose one chunk: the whole behavior as one diff, up to about 100
changed lines, give or take. Put related changes in the same chunk, such as the code, its caller,
and its test, with several hunks and files when the behavior needs them, so that the user reviews
them in one read. Do not pad a small behavior to reach the size. Do not walk a behavior up in
steps of 10, 20, then 40 lines; go straight to one chunk. A behavior that needs much more than 100
lines is two behaviors, so split the list, not the chunk. Show three things and then stop:

1. one line of intent that names each input case the chunk handles, for example "reject an empty
   name (`""`)";
2. the exact target `file:line`;
3. the change as a unified diff in a `diff` fence, made by a CLI tool, never a prose
   description of the change.

To make the diff, `cp` the target file into the scratchpad, make the one edit on that copy, and
run `diff -u <target> <copy>`. Never write the whole proposed file out by hand; the copy plus one
edit types only the changed lines. For a new file, write it in the scratchpad and diff against
`/dev/null`. Paste the output as is; the `-` lines show what goes away and the `+` lines show
what comes in. Keep the diff to the hunks of the chunk.

A chunk touches only what its behavior sentence names. No new helper, field, export, flag, case, or
todo goes in unless the sentence names it. In two real runs the user removed handling the behavior
did not ask for, a cache key and an edge case they called useless. A case that the sentence does not
name, such as a whitespace-only or very long value next to "reject an empty name", is a question in
one line under the diff, never code in it. Before you write a helper, search the repository for one
that does the same job, and call it. Apply the kit block `blocks/checked-units/SKILL.md` for a new
symbol and its caller. An edited function stays where it is, so the diff shows an edit, not a delete
plus an add; a move is its own chunk.

Code stays inline by default. Before a chunk adds or splits out a function, file, or module, apply
the kit block `blocks/when-to-extract/SKILL.md`, and name the reason that holds in the
intent line.

A file delete, a rename, or a merge-conflict resolution is its own chunk and never rides along
with an edit. Its diff is `git diff --stat` for that change plus the file list, and for a
conflict the resolved hunk from `diff -u`. It waits for `approve` like any other chunk.

Then wait for one of three replies:

| Reply | Result |
|---|---|
| `approve` | apply the chunk and run the smallest check for the touched unit (compile, lint, or one test). It proves only that chunk. The behavior proof follows `blocks/prove-it-works/SKILL.md` at the last chunk. During the check, prepare the next chunk's diff in scratch from the patched file. After the check passes, show its result in one line and the next chunk's diff in the same reply. When the chunk is the last one of its behavior, also commit the behavior and add the short hash to that line. If the check fails, show the failure, drop the prepared chunk, skip any commit, and wait for `change` or `deny` |
| `change: <text>` | revise the same chunk and show it again |
| `deny` | drop the chunk without argument, ask at most one question, then propose the next chunk |

The check line is the check's own output, such as `1 passed`, and `1 passed, a1b2c3d` after a
behavior commit.

Save the `diff -u` output to `<scratchpad>/chunk-<N>.diff`. On `approve`, apply it yourself with
`patch <target> <scratchpad>/chunk-<N>.diff`. Never spawn a chunk writer or any other worker; the
diff already is the change, and a worker would only re-type it. Pair mode is an explicit exception
to a host contract that sends edits to a coding seat.

Never apply a chunk before `approve`. Never bundle two chunks into one proposal. Exactly one chunk
stands in front of the user at a time, however many agents or panes are working; the others wait in
the task file.

## Commit per behavior

The `approve` of a behavior's last chunk is the commit order. Earlier chunks of the behavior stay
uncommitted in the working tree, so one behavior is one commit. The agent writes the message: a subject of one line in the
imperative, under 60 characters, that names the file or unit and the change; a body is optional
and goes in only when the chunk cut a corner or rejected an option, in two sentences at most.
Never ask "yes to commit" and never ask the user for a commit message.

## Behavior done

When every chunk of the current behavior is applied and its check passes, show, in this order:

1. one normal example, with its input and its result;
2. one failure example;
3. the test command and its result.

Then propose the first chunk of the next behavior. No commit question; the behavior commits
are the record. After the last behavior, go to Verify below. Inside an `implement` run, print the
implement workflow's `Next:` line for the review step and stop; do not start the review. There is
no cleanup gate, no readability review, and no extra tooling to set up or tear down.

## Verify, review, ship

After the last behavior of a standalone run:

1. Run the acceptance example on the real build through the user entry point. Show the revision
   (short hash), the action, and the observed output or screenshot path. When no tool can drive the
   product, say so and name the nearest check.
2. Ask for one fresh review seat on `Base..HEAD`, one round, by the small route's review rule in
   `skills/kit/skills/implement/references/small.md`, steps 4-5. Each fix is an approved chunk. A
   second `REQUEST CHANGES` goes to the user. When the session cannot start a seat, say
   `needs review seat: <base>..<head>` and stop there.
3. The user ships. `Status: active` stays until the user says so.

When Verify or review shows a wrong intent, go back to Align and amend the A-line. When the intent
is right and the code is wrong, it is a fix chunk.

## Guards

- Update the task file after every approved chunk.
- Keep replies to the chunk itself. No progress essays, no restated plan, and never a file path
  in place of the list or a chunk.
