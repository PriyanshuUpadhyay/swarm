---
name: web-search
description: "Wide web search by three visible seats on separate sites, which dig past the known answers and report to files. Use when a research step needs forums or community views, or this session's fetches are blocked."
license: MIT
metadata:
  kind: step
  owns: "the wide web search by visible seats, its brief, gate, and merge"
  serves: "research"
  needs: "a question with its sub-questions, known list, and seeds"
  gives: "a merged web report with a coverage table, and the seat reports"
  conflicts: ""
  users: "research, planner"
  source: "skills/research/SKILL.md (03-web triggers)"
---

## Rules
Three visible seats search three different parts of the web at the same time, each with its own
tools, and each writes its findings to a file. The chair merges them into one report. One session's
blocked fetch is another seat's open page, so the coverage is wider than any single session.

`blocks/run-workers/SKILL.md` selects the host and its reference; web search names no host. Without a visible host, stop and say so; a hidden or headless seat is not a substitute. When no user can answer that stop or another owner gate, apply `blocks/worker-brief/SKILL.md`. The default is to search with this session's tools and report the missing seat coverage.
- The kit root is two folders up from this file.
- Run the wide search when any of these is true; otherwise write in the step which trigger did not hold:
  - the user says intensive, wide, forums, or community;
  - the question is about practice or opinion rather than fact;
  - this session's own fetches are blocked;
  - the local step or a first web pass returned only the answers the user already knew.

**Start or continue.**

A web search is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. Choose the web-search run and report paths by
`references/run-folder.md`; the run folder also keeps the seat reports at the end.
"web-search continue <folder>" picks it up. Make the folder with
`python3 <kit root>/references/step_run.py start <folder> <kit root>/blocks/web-search/SKILL.md`, and close each step with `done`.

| File | Needs | Holds |
|---|---|---|
| `01-brief.md` | none | the sub-questions, the known list, the seeds, the reach rows that returned content, and the brief path |
| `02-seats.md` | brief | each seat's name, model, and report path, and whether its artifact was accepted |
| `03-gate.md` | seats | the depth-gate result per seat as each report arrives, any seat sent back, and the link check |
| `04-merge.md` | gate | the merged report path |
| `05-close.md` | merge | the seats closed and verified, and the scratch run directory removed |

For the default route, record `default taken` in `01-brief` and use `skip` on `02-seats` with that reason. Check this session's findings in `03-gate`, merge them in `04-merge`, and record that no seats or launch scratch were created in `05-close`.

**Seat mechanics.**

The chair runs the seats as the kit block `blocks/run-workers/SKILL.md` says, on the cheap
`search.web` route, and every seat is a visible pane. Share one brief. Completion on swarm is the
seat's ring and its report file.

Record the actual routed models as `blocks/run-workers/SKILL.md` requires. The site-set seat
names below describe search purposes, not provider identities.

These four details differ from a council run.

- Agent names are `ws-<voice>-<run>`, so a search and a council can run at the same time.
- Use the web-search launch scratch in `references/run-folder.md` as every seat's `--cwd`. The launcher
  pre-trusts only folders under its trust roots, so a seat started anywhere else stops on the Codex
  or AGY trust dialog. Never use `~/.swarm/runs`, because that is swarm's own message store.
- The seats resolve `search.web`, not `council.*`; the council keeps `council.gpt`,
  `council.gemini`, and `council.claude`.
- A Codex `search.web` seat gets `-- -c features.network_proxy.allow_local_binding=true` at
  `swarm launch`, so its browser rows reach the local `playwriter` relay. No other Codex seat gets
  this flag.

**Default site sets.**

| Seat | Sites |
|---|---|
| forums | Reddit — name the exact subreddits in the brief — the OpenAI Developer Community, the Cursor forum, and other Discourse forums |
| code/long-form | GitHub issues and discussions, blogs and newsletters, X and Bluesky through their search |
| independent communities | Hacker News through the Algolia API sorted by date, Lobsters, Tildes, Mastodon, Marginalia Search, and the blogs those threads link to |

The caller may replace any of these sets. Write the set each seat gets into that seat's brief.
Every seat also gets one non-SEO index for the long tail: Marginalia Search, Kagi Small Web, or
Exa "find similar" from a primary source, whichever its tools can reach.

**Reach tools.**

`scripts/reach.py` holds the known route for each site and its fallbacks, in a fixed order. It
prints the first route that gives content, logs each route that failed, and ends with a `blocked:`
line when all fail, so that line is the seat's blocked log. Before the chair writes the brief, it
runs each row below once with a short query from the question, and keeps only the rows that return
content. Then the chair pastes the kept rows into every seat's brief word for word, because a
paraphrase loses the exact commands and the seat stops at the first 403.

```
Search:  python3 <kit root>/blocks/web-search/scripts/reach.py search <site> "<query>" [--days N]
         site: hn, reddit, x, lobsters, github, youtube, web
Page:    python3 <kit root>/blocks/web-search/scripts/reach.py read "<url>"
         an X post, a Reddit thread, a login wall, a JS-only page, an arXiv abstract
Paper:   python3 <kit root>/blocks/web-search/scripts/reach.py paper <arxiv id> refs|cites
         the papers it cites, or the papers that cite it, for origins and forward search
YouTube: yt-dlp --write-auto-sub --skip-download -o "/tmp/%(id)s" "<url>"
```

Write `<kit root>` as the kit's real folder in the brief. The browser route (Reddit, X, Lobsters,
and pages that block a reader) needs `playwriter` on the PATH, its extension on in the user's
browser, and a seat that may reach local addresses. The Seat mechanics section names the launch
flag for a Codex seat, whose sandbox blocks them. The routes only read, so a seat never signs in,
posts, likes, follows, or types into a page there. The Reddit and X rows try the pinned last30days
engine first (`~/.local/share/swarm-vendor/last30days-v3.31.0`, never edited) and use the browser
when it gives nothing.

**Depth, a hard rule.**

The value of a seat is what sits below the famous answers. Every brief gives the kit block
`blocks/deep-search/SKILL.md` with the known list and the seeds filled in, and adds the two seat
rules below. A seat of another provider than the chair gets each named block's Rules quoted, as
`blocks/worker-brief/SKILL.md` says; a seat of the chair's provider gets the block paths. The chair enforces all of them when each report arrives. The chair takes the seeds
from prior reports and its own knowledge at brief time. For later seeds from local sources, follow
the `03-web` section of `skills/research/SKILL.md`.

1. **Yield** — at least five findings that are not on the known list, each with evidence, one
   honest weakness, a source-agreement tag by `blocks/cite-sources/SKILL.md`, and one line on why
   it is under-discussed (new, niche, no marketing, non-English community). Attribute and count
   sources by that same block. Fewer is a Coverage note that names the searches tried, never "nothing found".
2. **Floor** — say in Coverage how many threads or pages from the seat's own site set it read. A
   seat that read fewer than ten names why its site set ran dry.

**The shared brief.**

Apply `blocks/worker-brief/SKILL.md` for common fields and read/write limits. One brief has one
section per seat, with these search-specific fields:

1. the question, verbatim, and its sub-questions, split by the chair as `blocks/deep-search/SKILL.md`
   says;
2. the site set for that seat;
3. an "already cited, do not re-cite" list;
4. **depth** — the deep-search block and the Yield and Floor rules above, given as the Depth rule
   says, with the known list and the seeds filled in;
5. **method** — use your own native search and fetch tools and the Reach tools block; when a
   site blocks you, run the `reach.py` row for it, then try the search snippet, a public mirror,
   and `archive.ph`; cite as `blocks/cite-sources/SKILL.md` and its parent
   `blocks/sources/SKILL.md` say; when the question is about how or why something works, follow
   each lead to its origin as `blocks/primary-sources/SKILL.md` says; stop after
   about 20 minutes;
6. **blocked log** — a seat may call a site blocked only when its Coverage lists, for that site,
   each step it tried and the error each step gave, as the `blocked:` line of `reach.py` gives it,
   for example `blocked: reddit: last30days → no results; reddit_browser → empty or bot wall`;
7. **output shape** — `Coverage` with a matrix row per sub-question (sources found, gap or not),
   `Findings` with a quote or paraphrase plus its link and source-agreement tag, `Origins` with rows
   as `blocks/primary-sources/SKILL.md` says, `What is contested`, `Sources` newest first, under
   800 words, and a final `RESEARCH_DONE` line;
8. **layout** — the seat's report follows the section order in
   the kit's `references/plan-layout.md` where that file applies; quote the rule lines the seat
   needs, because Codex and AGY seats do not load it.

Use a literal result path (`blocks/worker-brief/SKILL.md`) under the run directory.

**Chair synthesis.**

Gate each seat's report when it arrives; do not wait for the other seats. Merge after the gate
has handled all three reports.

1. Apply the depth gate. Reject a report whose findings are all on the known list, that stops at
   "nothing found" without the searches tried, that is under the floor, that calls a site blocked
   without a blocked log, or whose matrix leaves a sub-question with no sources and no gap note.
   Send that seat back at once, once only, with the depth rules quoted. Accept what it returns,
   and say so in the coverage table. Record `sent back: <seat> 1`
   in `03-gate.md` and read that line before any resend, so a seat is never sent back twice.
   Apply `blocks/cite-sources/SKILL.md` to each claim's evidence. Drop a finding that repeats the
   brief's seed words without a page that says them.
2. Check every link as `blocks/link-check/SKILL.md` says.
3. Merge the three reports at the web-search report path in `references/run-folder.md`, in the same output
   shape, plus a coverage table with one row per seat: seat, model, sites, threads read, blocked,
   and a matrix with one row per sub-question: independent sources per seat, gap or covered.
4. Copy the three seat reports into the web-search run folder named by `references/run-folder.md`.
5. Close out the seats by `blocks/run-workers/SKILL.md`.
6. Remove the scratch run directory last.

**Second round.**

Run a second round only when the seats contradict each other on the answer, not to polish a report
they agree on. Give each seat the other two seats' findings and ask for its answer in under 350
words.

**Guards.**

- Use the report-only seat contract in `blocks/worker-brief/SKILL.md`.
- Every claim follows `blocks/cite-sources/SKILL.md` and its parent `blocks/sources/SKILL.md`.

## Example
Initial state: a research question about practice, with three seats briefed.
Action: the forums seat reads 19 Reddit threads, the code/long-form seat is blocked by Reddit
with a 403 and reads 18 GitHub issues, and the independent communities seat reads 12 Hacker
News and Lobsters threads. The chair applies
`blocks/link-check/SKILL.md` to those links and puts the counts in the coverage table.
Result: the merged report has three coverage rows, and the code/long-form row names Reddit as blocked with its blocked log.

## Check (script)
- none: the seat reports and the merged report live in the run folder, which differs for each run.

## Check (judge)
- Does each accepted seat report meet the Yield and Floor rules? Quote its finding count, its thread count, and any `sent back:` line.
- Does the merged report have a coverage row per seat and a matrix row per sub-question? Quote both tables.
