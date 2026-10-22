---
name: research
description: Research one question and write a short evidence-backed report. Use when the user says research, look into, find out, compare, find me, writeups, articles, or good reads, asks what others do, or says "research continue".
---

# Research

Answer one question from evidence. Keep the report as a file. Apply the loaded agent instructions
to the reply. The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.

## Start or continue

Research is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. `references/run-folder.md` owns the research run and
report paths. "research continue <folder>" picks up that run.

Run the front door in `skills/plan/SKILL.md` with the preset below, and write `plan.md` in the
research run folder. When local facts set the web query, set `03-web.md`'s Needs cell to `local`;
otherwise keep `question`. Close each step with
`python3 <kit root>/references/step_run.py done <folder> <step> <n>`. The saved `plan.md` keeps this
run's chosen inputs.

## Preset

Stop: owner
Offers: decision-with-default, when-to-diagram, html-diagram, scratch-home

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| `01-question.md` | none | the question in one line and the decision it serves in one line | requirements | the question and its decision frame the run |
| `02-local.md` | question | what the local sources say, with paths | local-research | local sources come first |
| `03-web.md` | question | the `web-search` run folder, or the reason no trigger below held; a `Fetched:` list of URLs whose page content was read; the origin rows from `blocks/primary-sources/SKILL.md`, or `skipped: <reason>`; what the web says, with links to the sources that own the claims; the off-path ideas and the searches that found them | deep-search, primary-sources, cite-sources | the web part goes past the known answers |
| `04-test.md` | local, web | for a tool pick, each candidate's install, task, time, and result; else `skipped: not a tool pick` | tool-trial | a tool pick needs a hands-on test |
| `05-report.md` | local, web, test | the report path and the result of the link check | report, link-check | the report keeps the answer with checked links |
| `06-close.md` | report | the reply as sent | voice-reply | the owner reads the answer |

## 01-question

Write the question in one line, and the decision it serves in one line. Show both and go on. If two
readings of the question lead to different work, set the step to `waiting` with one question.

## 02-local

Do this step as the kit block `blocks/local-research/SKILL.md` says, and carry each decision's open
question, evidence, contrary evidence, and gap into the report.

## 03-web

Start this step when its Needs in `<folder>/plan.md` are done. With `question` as its input,
start the web work after `01-question.md` by the host split below, then do `02-local`
while it runs. With `local` as its input, use the local findings in the first brief. Brief seeds
also come from prior reports and the chair's knowledge. A later local seed goes as one follow-up
ask only when it changes the search.

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

Start it as its own step run with this run's slug, at the web-search path in
`references/run-folder.md`. Close `03-web.md` when its
merged report is written. Its report is one source among the others, not the answer. If a trigger
holds but this session cannot open visible seats, do the web part yourself and write in the gaps
that `web-search` did not run and why. When no user can answer a web-seat gate,
`blocks/worker-brief/SKILL.md` applies, and this is its default.

When the question is about how or why a technique, protocol, or implementation works, also apply
the kit block `blocks/primary-sources/SKILL.md`. Example. The question is about SQL databases.
A blog is the lead, and the answer rests on the paper it names.

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

Write the report at the research path in `references/run-folder.md` as the kit block
`blocks/report/SKILL.md` says, and
check every link as `blocks/link-check/SKILL.md` says before you finish.

## 06-close

Send the reply by the loaded agent instructions. The kit's `references/plan-layout.md` owns the
first screen, `blocks/report/SKILL.md` owns the report sections, and
`references/run-folder.md` owns its file path.

## Guards

- Every claim follows `blocks/cite-sources/SKILL.md`.
- The source blocks that this skill names share the rules in their parent `blocks/sources/SKILL.md`. Apply it with each of them.
- Do not edit any repository. This skill writes only its own report and its run folder.
