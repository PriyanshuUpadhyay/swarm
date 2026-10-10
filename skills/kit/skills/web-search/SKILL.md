---
name: web-search
description: Wide web search by three visible seats (GPT, Gemini, Claude) that each search a different set of sites with their own tools and report to files. Depth is a hard rule, so every brief names the known answers as baseline only and the seats dig for under-discussed options. Use when one session's fetches are blocked or when the user asks for a wide, deep, or intensive search.
---

# Web search

Three visible seats search three different parts of the web at the same time, each with its own
tools, and each writes its findings to a file. The chair merges them into one report. One session's
blocked fetch is another seat's open page, so the coverage is wider than any single session.

The runtime adapter (`orchestrate-claude`) selects the host and its reference; web search names no host. Without a visible host, stop and say so; a hidden or headless seat is not a substitute.

## Start or continue

A web search is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. The run folder is
`~/.claude/reports/<YYYY-MM-DD>-<slug>-seats/`, which also keeps the seat reports at the end.
"web-search continue <folder>" picks it up. Make the folder with
`python3 <kit>/references/step_run.py start <folder> <this SKILL.md>`, and close each step with `done`.

| File | Needs | Holds |
|---|---|---|
| `01-brief.md` | none | the sub-questions, the known list, the seeds, the reach rows that returned content, and the brief path |
| `02-seats.md` | brief | each seat's name, model, and report path, and whether its artifact was accepted |
| `03-gate.md` | seats | the depth-gate result per seat as each report arrives, any seat sent back, and the link check |
| `04-merge.md` | gate | the merged report path |
| `05-close.md` | merge | the seats closed and verified, and the scratch run directory removed |

## Seat mechanics

The seats follow the council's mechanics but ride the cheap `search.web` route. Share one brief.
The host reference selected by the runtime adapter owns launch, completion, retry, and close-out
mechanics. Read and follow it; completion on swarm is the seat's ring and its report file.
The kit's `references/fan-out.md` owns fan-out policy.

These three details differ from a council run.

- Agent names are `ws-<voice>-<run>`, so a search and a council can run at the same time.
- The run directory is `~/.swarm/ws/<run>`, and every seat gets it as `--cwd`. The launcher
  pre-trusts only folders under its trust roots, so a seat started anywhere else stops on the Codex
  or AGY trust dialog. Never use `~/.swarm/runs`, because that is swarm's own message store.
- The seats resolve `search.web`, not `council.*`; the council keeps `council.gpt`,
  `council.gemini`, and `council.claude`.

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
runtime adapter names the launch flag for a seat whose sandbox blocks them. They only read, so a seat never signs in, posts,
likes, follows, or types into a page there. If `agent-reach` is not installed, say so in the
coverage table and use the fallback ladder in the method only.

## Depth, a hard rule

Search engines and answer engines rank the same famous names first, and the user already knows
those. The value of a seat is what sits below that surface. Every brief carries this section, and
the chair enforces it when each report arrives.

Example. The question is "which small model cleans up dictation text". A surface pass returns
Whisper and Qwen. The depth pass lists those two as "known, baseline only", seeds Moonshine,
SenseVoice, and Granite, and asks for people who replaced a general model with a small purpose-built
one. It returns fifteen candidates the surface pass missed.

The brief gives each seat:

1. **Sub-questions** — The chair splits the question into three to six sub-questions before any
   seat starts, because the first sources found are the most findable, not the most
   representative. Each seat reports per sub-question. A sub-question with fewer than two
   independent sources gets a written gap note, never a stretched finding.
2. **Known, baseline only** — the famous answers, by name. A seat reports each in one line as a
   comparison and never counts it as a finding.
3. **Seeds** — candidates the chair already suspects are under-discussed, so the seat starts below
   the surface. The chair takes them from prior reports and its own knowledge at brief time.
   For later seeds from local sources, follow the `03-web` section of `research`.
   An empty seed list is allowed only when the chair says why.
4. **How to dig** — sort by new, not top; read the comments, not only the post; follow a thread to
   the author's repo, model card, or paper; search the technique, format, and runtime names, not
   only the product name; on GitHub and Hugging Face sort by recently updated and read repos with
   few stars; search non-English communities when the topic has them.
5. **Yield** — at least five findings that are not on the known list, each with evidence, one
   honest weakness, a confidence tag (high: two independent primary sources agree; medium: mixed
   or secondary; low: one source or inference), and one line on why it is under-discussed (new,
   niche, no marketing, non-English community). Two sources that cite the same origin count as one
   source. A homepage or a product page is not evidence for a specific claim. Fewer is a Coverage
   note that names the searches tried, never "nothing found".
6. **Floor** — read at least ten threads or pages from the seat's own site set before the seat may
   stop, and say the count in Coverage. A seat that finishes in five minutes has not searched.

## The shared brief

One brief, one section per seat. It holds:

1. the question, verbatim;
2. the site set for that seat;
3. an "already cited, do not re-cite" list;
4. **depth** — the Depth section above, quoted in full, with the known list and the seeds filled
   in;
5. **method** — use your own native search and fetch tools and the Reach tools block; prefer
   2025-2026 material; when a site blocks you, try in order the Reach tool for that site, the
   browser row, the Jina reader, the search snippet, `old.reddit.com`, `<url>.json`, a public mirror,
   `web.archive.org`, and `archive.ph`; mark anything you could not open "(snippet only)"; never
   invent a quote; stop after about 20 minutes;
6. **blocked log** — a seat may call a site blocked only when its Coverage lists, for that site,
   each step it tried and the error each step gave, for example
   `reddit: browser_read → no extension; r.jina.ai → 403; old.reddit.com → 403; archive.ph → no copy`;
7. **output shape** — `Coverage` with a matrix row per sub-question (sources found, gap or not),
   `Findings` with a quote or paraphrase plus its link and confidence tag, `What is contested`,
   `Sources` newest first, under 800 words, and a final `RESEARCH_DONE` line;
8. **layout** — the seat's report follows the section order in
   `~/.claude/references/plan-layout.md` where that file applies; quote the rule lines the seat
   needs, because Codex and AGY seats do not load it;
9. the literal response path for that seat under the run directory, never a `$VAR`.

## Chair synthesis

Gate each seat's report when it arrives; do not wait for the other seats. Merge after the gate
has handled all three reports.

1. Apply the depth gate. Reject a report whose findings are all on the known list, that stops at
   "nothing found" without the searches tried, that is under the floor, that calls a site blocked
   without a blocked log, or whose matrix leaves a sub-question with no sources and no gap note.
   Send that seat back at once, once only, with the Depth section quoted. Accept what it returns,
   and say so in the coverage table. Record `sent back: <seat> 1`
   in `03-gate.md` and read that line before any resend, so a seat is never sent back twice.
   Mark "(unverified)" and drop
   from the answer any finding whose only evidence is a homepage or that repeats the brief's seed
   words without a page that says them.
2. Check every link as the link check in `research` says. Mark a link only a seat could open as
   "(seat-read only)".
3. Merge the three reports into `~/.claude/reports/<YYYY-MM-DD>-<slug>-web.md`, in the same output
   shape, plus a coverage table with one row per seat: seat, model, sites, threads read, blocked,
   and a matrix with one row per sub-question: independent sources per seat, gap or covered.
4. Copy the three seat reports next to it, under
   `~/.claude/reports/<YYYY-MM-DD>-<slug>-seats/`.
5. Close the seats and verify their removal.
6. Remove the scratch run directory last.

Example. GPT reads 19 Reddit threads, GEMINI is blocked by Reddit with a 403 and reads 18 GitHub
issues, CLAUDE reads 12 Hacker News and Lobsters threads. The chair cannot open Reddit either, so
every Reddit claim is marked "(seat-read only)" and the merged report says so in its coverage table.

## Second round

Run a second round only when the seats contradict each other on the answer, not to polish a report
they agree on. Give each seat the other two seats' findings and ask for its answer in under 350
words.

## Guards

- Seats are report-only. They write their own report file and nothing else.
- Workers follow the active runtime adapter's `Worker contract` section.
- Seats never read `~/.claude/history.jsonl`, `~/.claude/sessions/`, `~/.claude/projects/`,
  `~/.claude-*`, `~/.codex-*`, or `~/.gemini`.
- Never invent a quote, and never present a search snippet as a read page.
