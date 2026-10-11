---
name: review-walk
description: Walk a GitHub PR with the user in five steps (scope, map, check, comments, walk), one status file per step under tmp/review-walk/ in the repo. The check is a review-check run; this skill adds comments in the reviewer's own voice in a pending GitHub review that the reviewer submits, and in a new round shows each file's diff since the reviewer last viewed it. Use for "review pr" with a PR number or "review-walk continue" with a run when the user reviews a PR themselves. flow, deliver, and pair call review-check instead.
---

# review-walk

Example. `review-walk pr 42` makes `<repo>/tmp/review-walk/pr-42-01/` with five step files.
The agent maps the files into groups, runs review-check on the PR range, and drafts 15
short comments in your voice. You say yes, and they go into your pending review on GitHub. You walk
the PR there, mark files viewed, edit or delete drafts, and submit. The author pushes again.
`review-walk pr 42` then makes `pr-42-02`, which shows only what changed in each file after you
viewed it, and marks the unchanged files viewed again on GitHub after you say yes.

## Contract

- The review is report-only. Never edit, commit, stash, switch, or reset the user's checkout.
  `git fetch` gets objects and `FETCH_HEAD` only.
- Never submit or publish a review. The user does it on GitHub.
- Every GitHub write (`post`, `remark`) needs the user's yes in chat for that run.
- review-check owns the findings, the lens, the repo rules, and the verdict. This skill never
  states a verdict that review-check did not print.
- No findings about secrets in plain text in a public comment. Tell the user in chat instead.
  `post` refuses a body with a known token or key shape, but that check is partial, so read each
  body for other secrets too.

## Run folder

`<repo-root>/tmp/review-walk/<run>/`. `<run>` is `pr-<n>-NN`, `local-NN`, or
`range-<base7>-<head7>-NN`, where `NN` counts rounds of the same target. The script adds `/tmp/`
to `.git/info/exclude`, so no one else sees the folder.

```sh
W=<skill-dir>/scripts/review_walk.py
python3 $W start <pr-number|url|local|base..head>   # makes the run and writes 01-scope.md
python3 $W status [<run>]                            # status, ready, stale, and each problem it can prove
python3 $W post <run> <run>/comments.json            # drafts into your pending review
python3 $W viewed <run>                              # records viewed marks for the next round
python3 $W remark <run>                              # marks unchanged-since-view files viewed
```

Run the script from the repo root. To continue in a new chat, run `status` and take the first
ready step.

## Status line

A review walk is a step run, so the kit's `references/step-run.md` owns the status line, ready
and stale steps, pick-up, close, and when a step waits for the user. The script reads the same
status line.

## Steps

| File | Who | Uses | Holds |
|---|---|---|---|
| `01-scope.md` | script | none | target, base, head, every file with its state: new to you, changed since view, unchanged since view, whitespace only |
| `02-map.md` | chair | 01 | the intent, files in groups by feature in reading order with one cited line of purpose per group, the critical path, and a diagram when the change needs one |
| `03-check.md` | chair | 01 | line 3 of the review-check `03-verdict.md`, then `Run: <its run folder>` |
| `04-comments.md` | chair | 02, 03 | the verdict, and the comments in the reviewer's voice |
| `05-walk.md` | user, chair | 04 | viewed marks, the user's own comments, what was submitted |

1. **Scope.** Run `start`. In a new round, review only the files that are new to you or changed
   since view. If `unchanged.txt` is not empty, tell the user the count and run `remark` after a
   yes.
2. **Map.** Put every file of `01-scope.md` into exactly one group. Use the PR body and the code
   to name each group, for example `s1 storage`, `s2 api routes`, `mig migrations`. Order the
   groups so a reader meets a type before its users. End with `Files: <mapped> of <total>`, and
   the two numbers must be equal. Mark lock files, generated code, and vendored code
   `skipped: generated` in their group, and do not explain them.

   Above the groups, write the intent in one or two lines, the problem and the approach, from the
   PR body. If the body does not say, write `intent not stated; inferred from <path:line>`. Below
   the groups, trace one critical path, the most important changed behavior from input to output,
   with a `path:line` for each hop and each guard on the way.

   Add a diagram when the kit's `references/explain-formats.md` says that one helps, and draw it
   as that file says for the place it is read.

   Cite new-side lines with the repo-relative path, and cite a removed line as `deleted`. Then run
   `python3 <skill-dir>/scripts/validate_citations.py <run>/diff.patch <run>/02-map.md`. Fix or
   drop each citation that it rejects, because a wrong citation looks as sure as a right one. The
   map explains and never judges. Findings belong to step 03.
3. **Check.** Run review-check on `<Base>..<Head>` from `01-scope.md`, or `local` for a local
   target. In a new round, add `--patch <run>/since-view.patch`, which holds what changed since
   view and the whole diff of each file new to you. Follow review-check to its verdict. Line 3 of
   `03-check.md` is line 3 of its `03-verdict.md`, and line 4 is `Run: <its run folder>`, relative
   to the repo root. An `INCOMPLETE` verdict keeps this step open. `status` prints a problem when
   the map skips a file or names one twice, or when line 3 does not match a done review-check run
   of this head.
4. **Comments.** Take the `fix`, `ask`, and `note` rows of the review-check run. Drop what the user
   already said in an earlier round or in a pending comment. Write each comment in the voice of
   `~/.review-walk/voice.md`, with the file-level or line-level choice it gives. With no voice file,
   write short plain comments: one clause, a question when the fix is a guess, no praise, no
   severity labels, and "potential issue, not confirmed:" before an unproved defect. Write
   `comments.json` as a list of `{path, body, first, last}`, where `first` and `last` are exact
   quotes from the new side and a file-level comment has no `first`. A row that quotes a removed
   line becomes a file-level comment.
   Line 3 of `04-comments.md` is the verdict line of `03-check.md`. Show the comments in chat,
   grouped by the groups of step 02. Run `post` only after the user says yes.
5. **Walk.** The user reads the PR on GitHub, edits or deletes drafts, adds their own comments,
   marks files viewed, and submits. Then run `viewed` and write in `05-walk.md` the count of viewed
   files and whether the review was submitted. For a local or range target, this step is skipped.

## Roles

The chair does every step, because steps 01, 02, 04, and 05 need the user. Step 03 uses
review-check's seats, as review-check's `orchestration.json` declares.

## Voice

`~/.review-walk/voice.md` holds how the reviewer writes comments, from their own past reviews. It
stays out of this public skill. The repo rules belong to review-check.

## Callers

flow, deliver, and pair call review-check, not this skill.

## Output

In chat, give the intent, the map with its diagram, and the verdict with its counts, then the
comments by group, each with its `file:line`.
Then give the run folder and the next step.
