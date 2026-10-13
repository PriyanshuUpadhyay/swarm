---
name: research
description: Research one question and write a short evidence-backed report, shown in full in the reply and kept as a file. Use when the user says research, look into, find out, compare, find me, writeups, articles, good reads, or asks what others do, and for "research continue" with a folder. Calls the `web-search` skill for a wide forum search. A Fable chair sends the web part to a `search.web` seat.
---

# Research

Answer one question from evidence. Keep the report as a file, and put the whole report in the
reply. The kit's blocks are in `~/.claude/blocks/`, a link to the kit's `blocks/` folder.

## Start or continue

Research is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. "research <question>" starts a run in
`~/.claude/reports/<YYYY-MM-DD>-<slug>/`. "research continue <folder>" picks it up. Make the folder with
`python3 ~/.claude/references/step_run.py start <folder> <this SKILL.md>`, and close each step with `done`.

| File | Needs | Holds |
|---|---|---|
| `01-question.md` | none | the question in one line and the decision it serves in one line |
| `02-local.md` | question | what the local sources say, with paths |
| `03-web.md` | question | the `web-search` run folder, or the reason no trigger below held; a `Fetched:` list of URLs whose page content was read; what the web says, with links to the sources that own the claims; the off-path ideas and the searches that found them |
| `04-test.md` | local, web | for a tool pick, each candidate's install, task, time, and result; else `skipped: not a tool pick` |
| `05-report.md` | local, web, test | the report path and the result of the link check |
| `06-close.md` | report | the reply as sent |

## 01-question

Write the question in one line, and the decision it serves in one line. Show both and go on. If two
readings of the question lead to different work, set the step to `waiting` with one question.

## 02-local

Do this step as the kit block `blocks/local-research/SKILL.md` says, and carry each decision's open
question, evidence, contrary evidence, and gap into the report.

## 03-web

Start the web seat as soon as `01-question.md` is written, then do `02-local` while it runs.
Brief seeds come from prior reports and the chair's knowledge. A seed found in `02-local` goes
to the seat as one follow-up ask only when it changes the search.

On continue, skip only URLs on the `Fetched:` list with saved evidence. Refresh a source when the
question or final link check needs current content.

Under a Fable chair, one `search.web` seat does the web part and writes its notes to
a file; any other session uses its own search and fetch tools. Either way the web part follows the
kit block `blocks/deep-search/SKILL.md` and the blocked-site ladder (the brief's **method** item)
in `web-search`.

Invoke the `web-search` skill when any of these is true:

- the user says intensive, wide, forums, or community;
- the question is about practice or opinion rather than fact;
- this session's own fetches are blocked;
- the local step or a first web pass returned only the answers the user already knew.

Start it as its own step run with this run's slug, so its folder
`~/.claude/reports/<YYYY-MM-DD>-<slug>-seats/` sits next to this one. Close `03-web.md` when its
merged report is written. Its report is one source among the others, not the answer. If a trigger
holds but this session cannot open visible seats, do the web part yourself and write in the gaps
that `web-search` did not run and why.

Every run also searches off the usual path, as `blocks/deep-search/SKILL.md` says. Example. The
question is "which state library should this React app use". The usual answers are libraries. An
off-path search also finds a team that removed its state library and kept the state in the URL, so
every view can be shared and reloaded.

For X, Reddit, and other pages that a plain fetch cannot open, use the browser rows of the Reach
tools in `web-search`. They read the page in the user's own signed-in browser. Each claim cites its
source as `blocks/cite-sources/SKILL.md` says.

## 04-test

When the question picks a tool, library, or service for later use, test the candidates as the kit
block `blocks/tool-trial/SKILL.md` says.

## 05-report

Write `~/.claude/reports/<YYYY-MM-DD>-<slug>.md` as the kit block `blocks/report/SKILL.md` says, and
check every link as `blocks/link-check/SKILL.md` says before you finish.

## 06-close

Send the reply as `blocks/report/SKILL.md` says: the first screen, the whole report, and its path.

## Guards

- Every claim follows `blocks/cite-sources/SKILL.md`.
- Do not edit any repository. This skill writes only its own report and its run folder.
