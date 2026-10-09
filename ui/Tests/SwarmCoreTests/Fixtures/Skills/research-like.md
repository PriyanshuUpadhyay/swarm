---
name: research
description: Research one question and write a short evidence-backed report, shown in full in the reply and kept as a file. Use when the user says research, look into, find out, compare, find me, writeups, articles, good reads, or asks what others do, and for "research continue" with a folder. Calls the `web-search` skill for a wide forum search. A Fable chair sends the web part to a `search.web` seat.
---

# Research

Answer one question from evidence. Keep the report as a file, and put the whole report in the
reply.

## Start or continue

Research is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. "research <question>" starts a run in
`~/.claude/reports/<YYYY-MM-DD>-<slug>/`. "research continue <folder>" picks it up. Make the folder with
`python3 <kit>/references/step_run.py start <folder> <this SKILL.md>`, and close each step with `done`.

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

Read these sources as the kit's `references/fan-out.md` says. When they disagree, trust them in
the following order.

1. the repository at hand;
2. its docs;
3. prior reports under `~/.claude/reports/`;
4. council logs under `~/dotfiles/docs/council/`.

For each decision, track the open question, primary evidence, contrary evidence, and remaining
gap in the report. Inspect the source's actual method or code before accepting its headline.
Separate independent origins from pages that repeat one origin. Page counts and agreeing agents
do not establish correctness. If evidence is thin, name the gap instead of making the claim
stronger; the user does not need to ask for a second, deeper pass.

## 03-web

Start the web seat as soon as `01-question.md` is written, then do `02-local` while it runs.
Brief seeds come from prior reports and the chair's knowledge. A seed found in `02-local` goes
to the seat as one follow-up ask only when it changes the search.

On continue, skip only URLs on the `Fetched:` list with saved evidence. Refresh a source when the
question or final link check needs current content.

Under a Fable chair, one `search.web` seat does the web part and writes its notes to
a file; any other session uses its own search and fetch tools. Either way the web part follows the
Depth rule and the blocked-site ladder (the brief's **method** item) in `web-search`: name the
known answers as baseline only, seed the under-discussed candidates, and dig below the first page
of results.

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

### Off the usual path

Every run also looks for ideas that the common advice misses or rejects. You do not know in advance
what you will find, so search on purpose:

- the opposite of the usual answer, such as "we removed X" or "X considered harmful";
- the same problem in another field or another community, which often solved it first;
- a way to drop the problem instead of solving it;
- old or abandoned tools whose idea is still good, and small projects with few stars;
- people who tried the popular answer and moved away, with their reason.

Example. The question is "which state library should this React app use". The usual answers are
libraries. An off-path search also finds a team that removed its state library and kept the state
in the URL, so every view can be shared and reloaded.

Keep each off-path idea that has a real source, even when it looks wrong at first. Write why it
might work and why it might fail. Do not let it replace the main answer without evidence.

For X, Reddit, and other pages that a plain fetch cannot open, use the browser rows of the Reach
tools in `web-search`. They read the page in the user's own signed-in browser.

Follow each claim back to the source that owns it, such as the official docs, the source code, the
spec, or the first-party API. A blog post, a forum answer, or a summary that reports the claim
points you to that source. It is not the evidence. When you cannot reach the owner, cite the
secondary page and mark the claim "(secondary)".

## 04-test

When the question picks a tool, library, or service for later use, test the candidates on this
machine. Make a shortlist of at least three. Install each one in a scratch place, run the same real
task on each, record time and success, and remove the ones you do not pick. A candidate that is not
installed stays on the list, because install time is not a reason to drop it. A slower, tested
answer is preferred over a fast pick from reading alone. Ask before an install that changes state
outside a scratch place, such as a GUI permission or a system service.

## 05-report

Write `~/.claude/reports/<YYYY-MM-DD>-<slug>.md` in plain words. Aim for about 800 words, and go
longer when the evidence needs it. Never cut a claim, a caveat, or a source only to meet the number.
Use this order:

1. **Answer** — the answer first, in two or three sentences.
2. **Evidence** — each claim with its link to the source that owns it.
3. **What is contested** — where the sources disagree.
4. **Off the usual path** — each off-path idea with its source, why it might work, and why it
   might fail. If none held up, name the searches you tried.
5. **Sources** — newest first, with dates.

Add a diagram or a local HTML page only as `~/.claude/references/explain-formats.md` says.

Check that every link resolves before you finish. An HTTP status shows only that the page opens,
so also find the claim's number or wording on the page you cite. Mark a link that does not resolve
or does not support its claim, and drop the claim that rests on it alone. When you convert a page,
PDF, or table to text, compare its headers, units, and values with the source and keep the page or
cell location in the citation. If the conversion lost structure, cite the original.

Example. The question is "does anyone run two coding agents on one repository". You read two prior
reports, then fetch six pages. One returns 404. The report keeps five links, marks the sixth
"(link dead)", and the reply shows the whole report.

## 06-close

Open the reply with the first screen in `~/.claude/references/plan-layout.md`. Then put the whole
report below it, from its `#` title down, and give the file path in one line.

## Guards

- Never invent a quote. Quote only text you read.
- Mark "(snippet only)" when a page could not be opened and only its search snippet was available.
- Prefer 2025-2026 sources. Say so when the best source is older.
- Do not edit any repository. This skill writes only its own report and its run folder.
