---
name: web-search
description: Wide web search by three visible seats (GPT, Gemini, Claude) that each search a different set of sites with their own tools and report to files. Depth is a hard rule, so every brief names the known answers as baseline only and the seats dig for under-discussed options. Use when one session's fetches are blocked or when the user asks for a wide, deep, or intensive search.
---

# Web search

Three visible seats search three different parts of the web at the same time, each with its own
tools, and each writes its findings to a file. The chair merges them into one report. One session's
blocked fetch is another seat's open page, so the coverage is wider than any single session.

The active runtime adapter selects the host and its reference; web search names no host. Without a visible host, stop and say so; a hidden or headless seat is not a substitute. When no user can answer that stop or another owner gate, apply `blocks/worker-brief/SKILL.md`. The default is to search with this session's tools and report the missing seat coverage.
The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.

## Start or continue

A web search is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. Choose the web-search run and report paths by
`references/run-folder.md`; the run folder also keeps the seat reports at the end.
"web-search continue <folder>" picks it up. Make the folder with
`python3 <kit root>/references/step_run.py start <folder> <this SKILL.md>`, and close each step with `done`.

| File | Needs | Holds |
|---|---|---|
| `01-brief.md` | none | the sub-questions, the known list, the seeds, the reach rows that returned content, and the brief path |
| `02-seats.md` | brief | each seat's name, model, and report path, and whether its artifact was accepted |
| `03-gate.md` | seats | the depth-gate result per seat as each report arrives, any seat sent back, and the link check |
| `04-merge.md` | gate | the merged report path |
| `05-close.md` | merge | the seats closed and verified, and the scratch run directory removed |

## Seat mechanics

The chair runs the seats as the kit block `blocks/run-workers/SKILL.md` says, on the cheap
`search.web` route, and every seat is a visible pane. Share one brief. Completion on swarm is the
seat's ring and its report file.

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

## Default site sets

| Seat | Sites |
|---|---|
| GPT | Reddit — name the exact subreddits in the brief — the OpenAI Developer Community, the Cursor forum, and other Discourse forums |
| GEMINI | GitHub issues and discussions, blogs and newsletters, X and Bluesky through their search |
| CLAUDE | Hacker News through the Algolia API sorted by date, Lobsters, Tildes, Mastodon, Marginalia Search, and the blogs those threads link to |

The caller may replace any of these sets. Write the set each seat gets into that seat's brief.
Every seat also gets one non-SEO index for the long tail: Marginalia Search, Kagi Small Web, or
Exa "find similar" from a primary source, whichever its tools can reach.

## Reach tools

Agent Reach installs command-line readers for sites that block a plain fetch. Before the chair
writes the brief, it runs every row below once in one command, with a short query from the question.
It keeps only the rows that return content. `agent-reach doctor` is not enough, because it skips live checks and
leaves Exa and GitHub unmarked even when they work. Then the chair pastes the kept rows into every
seat's brief word for word, because a paraphrase loses the exact commands and the seat stops at the
first 403.

```
Public page:  curl -s "https://r.jina.ai/<url>"
Browser page: python3 <skill-dir>/scripts/browser_read.py "<url>"
           reads any page in the user's own signed-in browser: X posts, Reddit threads, login walls, JS-only pages
Search:    mcporter call exa.web_search_exa query="<query>" numResults=10
Reddit:    python3 <skill-dir>/scripts/browser_read.py "https://www.reddit.com/r/<sub>/search/?q=<query>&sort=new"
           then the same command on each post URL, for the post and its comments
X:         python3 <skill-dir>/scripts/browser_read.py x "<query>"
           then the same command on each post URL, for the thread
YouTube:   yt-dlp --write-auto-sub --skip-download -o "/tmp/%(id)s" "<url>"
GitHub:    gh search issues "<query>" --sort updated --limit 30
```

Write `<skill-dir>` as this skill's real folder in the brief. The browser rows need `playwriter` on
the PATH, its extension on in the user's browser, and a seat that may reach local addresses. The
Seat mechanics section names the launch flag for a Codex seat, whose sandbox blocks them. They only read, so a seat never signs in, posts,
likes, follows, or types into a page there. If `agent-reach` is not installed, say so in the
coverage table and use the fallback ladder in the method only.

## Depth, a hard rule

The value of a seat is what sits below the famous answers. Every brief quotes the kit block
`blocks/deep-search/SKILL.md` in full, with the known list and the seeds filled in, and adds the two
seat rules below. The chair enforces all of them when each report arrives. The chair takes the seeds
from prior reports and its own knowledge at brief time. For later seeds from local sources, follow
the `03-web` section of `research`.

1. **Yield** — at least five findings that are not on the known list, each with evidence, one
   honest weakness, a source-agreement tag by `blocks/cite-sources/SKILL.md`, and one line on why
   it is under-discussed (new, niche, no marketing, non-English community). Attribute and count
   sources by that same block. Fewer is a Coverage note that names the searches tried, never "nothing found".
2. **Floor** — read at least ten threads or pages from the seat's own site set before the seat may
   stop, and say the count in Coverage. A seat that finishes in five minutes has not searched.

## The shared brief

Apply `blocks/worker-brief/SKILL.md` for common fields and read/write limits. One brief has one
section per seat, with these search-specific fields:

1. the question, verbatim, and its sub-questions, split by the chair as `blocks/deep-search/SKILL.md`
   says;
2. the site set for that seat;
3. an "already cited, do not re-cite" list;
4. **depth** — the deep-search block and the Yield and Floor rules above, quoted in full, with the
   known list and the seeds filled in;
5. **method** — use your own native search and fetch tools and the Reach tools block; when a
   site blocks you, try in order the Reach tool for that site, the
   browser row, the Jina reader, the search snippet, `old.reddit.com`, `<url>.json`, a public mirror,
   `web.archive.org`, and `archive.ph`; cite as `blocks/cite-sources/SKILL.md` says, with that
   block's Rules quoted; when the question is about how or why something works, follow each lead to
   its origin as `blocks/primary-sources/SKILL.md` says, with that block's Rules quoted; stop after
   about 20 minutes;
6. **blocked log** — a seat may call a site blocked only when its Coverage lists, for that site,
   each step it tried and the error each step gave, for example
   `reddit: browser_read → no extension; r.jina.ai → 403; old.reddit.com → 403; archive.ph → no copy`;
7. **output shape** — `Coverage` with a matrix row per sub-question (sources found, gap or not),
   `Findings` with a quote or paraphrase plus its link and source-agreement tag, `Origins` with rows
   as `blocks/primary-sources/SKILL.md` says, `What is contested`, `Sources` newest first, under
   800 words, and a final `RESEARCH_DONE` line;
8. **layout** — the seat's report follows the section order in
   the kit's `references/plan-layout.md` where that file applies; quote the rule lines the seat
   needs, because Codex and AGY seats do not load it.

Use a literal result path (`blocks/worker-brief/SKILL.md`) under the run directory.

## Chair synthesis

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

Example. GPT reads 19 Reddit threads, GEMINI is blocked by Reddit with a 403 and reads 18 GitHub
issues, and CLAUDE reads 12 Hacker News and Lobsters threads. The chair applies
`blocks/link-check/SKILL.md` to those links and puts the counts in the coverage table.

## Second round

Run a second round only when the seats contradict each other on the answer, not to polish a report
they agree on. Give each seat the other two seats' findings and ask for its answer in under 350
words.

## Guards

- Use the report-only seat contract in `blocks/worker-brief/SKILL.md`.
- Every claim follows `blocks/cite-sources/SKILL.md`.
