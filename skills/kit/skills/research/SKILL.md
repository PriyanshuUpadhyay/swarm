---
name: research
description: Research one question in a short report with checked links, covering recent underrated finds and the origin of each core claim. Use when the user says research, look into, or find out, or asks where a claim comes from.
---

# Research

Answer one question from evidence, in a short report with links that open. The kit root is two
folders up from this skill's folder. `reach` below means
`python3 <kit root>/blocks/web-search/scripts/reach.py`.

## Method

1. **Frame.** Write the question in one line and the decision it serves in one line. List the
   answers you already know. They are background, not findings.
2. **Local first.** Read earlier reports in `~/.claude/reports/` and the repository files that the
   question names, as `<kit root>/blocks/local-research/SKILL.md` says.
3. **Recent and underrated.** Set a window from the question, or 90 days. Search where
   practitioners write first.
   - `reach search <site> "<query>" --days N` for `hn`, `reddit`, `x`, `lobsters`, `github`,
     `youtube`, and `web`. Use two or three queries per site, one of them for the opposite view.
   - Forward search: `reach paper <arxiv id> cites` on the key paper finds newer work that builds
     on it.
   - Keep an item only when you opened it and its own page dates it inside the window.
4. **Origins.** For each core claim, find the paper, spec, or code that first made it. Read the
   exact section, and quote it with its location.
   - `reach paper <arxiv id> refs` walks back to older sources. `reach read <url>` opens a page that
     blocks a plain fetch.
   - Use the newest version of a paper. When versions give different numbers, say which version
     says what.
   - A blog, a summary, or an abstract is a lead, never the origin. Name it "(secondary)" when you
     cannot reach the origin.
5. **Check each number before you write.** Open the cited page again with `reach read` and compare
   the number, its unit, what it measures (odds or a rate), its scope (which files, models, or
   tasks), and the version. Fix or drop each claim that does not match.
6. **Write the report** as `<kit root>/blocks/report/SKILL.md` says: the answer in two or three
   sentences, then Evidence, What is contested, Off the usual path, and Sources newest first with
   dates. Keep process out of the report, so no search log, checklist, or run notes.
7. **Check every link** as `<kit root>/blocks/link-check/SKILL.md` says. A reader must be able to
   open each link without your browser. When a page opens only through the browser, such as an X
   post or a page that answers 403 to a plain fetch, cite a `web.archive.org` copy next to it, or
   drop the claim.

When `reach` prints a `blocked:` line, copy it into your notes, and name the gap in one plain
sentence in the report. Do not call a site blocked without that line.

## Example

Question: "A blog says LSM-trees were designed for SSDs. Is that right?" Step 4 finds the origin,
the 1996 paper by O'Neil and others, and reads its introduction, which frames the cost as disk arm
movement under high insert rates. The report quotes that passage with its section, and names the
blog as the lead.

## Full mode

When a swarm host can open visible panes and the question is about practice or opinion, run steps
3 and 4 with web seats as `<kit root>/blocks/web-search/SKILL.md` says. Step 5 still applies to every
number that the seats bring back.

## Start or continue

A planned run (`/swarm:swarm research ...`, or a plan with `Preset: research`) runs the
method above as the step run below.

Research is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. `references/run-folder.md` owns the research run and
report paths. "research continue <folder>" picks up that run.

Run the front door in `skills/swarm/SKILL.md` with the preset below, and write `plan.md` in the
research run folder. When local facts set the web query, set `03-web.md`'s Needs cell to `local`;
otherwise keep `question`. Close each step with
`python3 <kit root>/references/step_run.py done <folder> <step> <n>`. The saved `plan.md` keeps this
run's chosen inputs.

## Preset

Stop: owner
Offers: tool-trial, decision-with-default, when-to-diagram, html-diagram, scratch-home

When the question picks a tool, library, or service for later use, the planner adds a step with the
offered `tool-trial` block before `04-report`, so the candidates are tested hands-on. Leave it out otherwise.

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| `01-question.md` | none | the question in one line and the decision it serves in one line | requirements | the question and its decision frame the run |
| `02-local.md` | question | what the local sources say, with paths | local-research | local sources come first |
| `03-web.md` | question | the `web-search` run folder, or the reason no trigger below held; a `Fetched:` list of URLs whose page content was read; the map table from `blocks/literature-map/SKILL.md`, or `skipped: <reason>`; the origin rows from `blocks/primary-sources/SKILL.md`, or `skipped: <reason>`; the recent-window rows from `blocks/deep-search/SKILL.md`; what the web says, with links to the sources that own the claims; the off-path ideas and the searches that found them | literature-map, deep-search, primary-sources, cite-sources, web-search | the web part maps the field, then goes past the known answers |
| `04-report.md` | local, web | the report path and the result of the link check | report, link-check, done-when-evidence | the report keeps the answer with checked links and a row per source type |
| `05-close.md` | report | the reply as sent | voice-reply | the owner reads the answer |

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

The `web-search` block names when the wide search runs. Start it as its own step run with this run's slug, at the web-search path in
`references/run-folder.md`. Close `03-web.md` when its
merged report is written. Its report is one source among the others, not the answer. If a trigger
holds but this session cannot open visible seats, do the web part yourself and write in the gaps
that `web-search` did not run and why. When no user can answer a web-seat gate,
`blocks/worker-brief/SKILL.md` applies, and this is its default.

When the question has a body of papers or reports behind it, map it first by
`blocks/literature-map/SKILL.md`, before the open search.

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

## 04-report

Write the report at the research path in `references/run-folder.md` as the kit block
`blocks/report/SKILL.md` says, and
check every link as `blocks/link-check/SKILL.md` says before you finish.

## 05-close

Send the reply by the loaded session reply rules. The kit's `blocks/report/SKILL.md` owns the report sections, and
`references/run-folder.md` owns its file path.

## Guards

- Every claim cites the source that owns it, as `<kit root>/blocks/cite-sources/SKILL.md` says.
- Do not edit any repository. Write only the report and your own notes.
- The source blocks named here share the rules in their parent `<kit root>/blocks/sources/SKILL.md`.
