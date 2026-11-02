---
name: literature-map
description: "Find a field's surveys first, date their scope, follow their citations back to the origin papers, and forward to newer work. Use when a question has a body of papers or reports behind it."
license: MIT
metadata:
  kind: step
  owns: "the survey-first pass of a research question and the shape of its map table"
  serves: "research, web search"
  needs: "a question with a body of papers, reports, or benchmarks behind it"
  gives: "a map table of surveys with their scope and date, the origin papers they name, and the newer work that cites them"
  conflicts: ""
  users: "research"
  source: "owner request 2026-11-01 (every research run covers the existing research, the source papers, and the new developments); the blind plan trial of 2026-11-01 (no block found surveys first, in its tasks T2, T3, and T5)"
---

## Rules
Find the surveys first
- Before the first open search, look for what already sums up the field: a survey or review paper, a systematic review, a benchmark paper, a vendor or lab overview, or a long report. Search `survey`, `review`, `systematic review`, `benchmark`, `taxonomy`, and `state of`, each with the field's own terms.
- For each survey, record its date, the period its sources cover, and its scope in one line. A survey older than the field's fast change is a map of the past, so say which part it no longer covers.
- With no survey found, write `no survey: <the searches tried>` and go on. The searches are the evidence.
Walk back
- Take the origin papers that two or more surveys cite for a core idea, and hand them to `blocks/primary-sources/SKILL.md`. That block owns how the origin is read and recorded.
Walk forward
- For the two to four most cited origins, look for newer work that cites them: the "cited by" list of a paper index, a recent survey's related work, or a search for the method's name after the newest survey's date. This finds what the surveys could not know yet.
- Hand each newer item to the recent window in `blocks/deep-search/SKILL.md`, which decides what to keep and why.
Record it
- Write one map table: survey or origin, year, scope or claim, link, and the newer work that cites it. Each row comes from a page you opened, as `blocks/cite-sources/SKILL.md` says.
When it does not apply
- Skip this block for a question with no body of papers or reports behind it, such as a current price, a browser support fact, or one product's settings. Write `skipped: <reason>` in the step file.

## Example
Initial state: the question is "how should agents compose workflows from small skills at run time".
Action: search `survey LLM agent planning`, `survey tool learning`, and `agent workflow generation benchmark`. Two surveys of 2024 and 2025 come back. Both cite ReAct and Toolformer as origins, so those go to `primary-sources`. A "cited by" search on the most cited origin, after the newest survey's date, finds three 2026 papers.
Result: a map table with the two surveys, their scope and date, the two origins, and the three newer papers, each with a link. The newer papers go to the recent window of `deep-search`.

## Check (script)
- none: the map table lives in the step file and the report, outside the commit range.

## Check (judge)
- Does the step name each survey found with its date and scope, or write `no survey:` with the searches tried? Quote the rows or the line.
- Does each origin that the surveys share go to `primary-sources`, and does a forward search follow the most cited origins? Quote one origin and one forward search.
- Was the block skipped for a question that has a body of papers behind it? Quote the `skipped:` line and the question, or write "not skipped".
