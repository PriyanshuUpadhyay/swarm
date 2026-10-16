---
name: worker-brief
description: "Write a worker's brief so it knows what it may change, what to check, where its result goes, and when to stop, and keep report-only seats from editing. Use when you send work to a seat or worker."
license: MIT
metadata:
  kind: rule
  owns: "what a worker brief holds and how a worker behaves"
  serves: "every step that sends work to a seat"
  needs: "a task for one seat"
  gives: "a brief that a seat can follow alone"
  conflicts: ""
  users: "run-workers, council, web-search, review-check, deliver"
  source: "skills/orchestrate-claude/SKILL.md (Worker contract and the report-only paragraph of all three adapters, moved here 2026-10-13); skills/council/SKILL.md (Pinned brief, report-only seat brief); skills/web-search/SKILL.md (Guards)"
---

## Rules
- Every brief says five things: what the worker may change, what it must check and how, what it leaves to another seat, where its result goes as a literal path (never a `$VAR`), and when it stops. Do not send a brief that misses one.
- A brief that asks for a judgment, such as a council, a search, or a review, also pins the question or the artifact word for word, the constraints the user settled, the exact files, URLs, or commits the seat may read, a line "do not look outside these", and the shape of the answer.
- Name the instruction and standards files the worker reads first, with the expected behavior and the checks. A worker does not inherit the chair's loaded rules. Point to a rule at its owner; do not copy its text into the brief. A seat of another provider gets this block's text in the brief, or as a copy pinned by its digest in the run folder.
- A report-only seat reads, and writes only its own result file. It does not edit a repository, apply a finding or a recommendation, commit, start another agent, or notify the user, even when its sandbox could write.
- A worker is a leaf. It never starts agents or panes, and never closes, sweeps, or drains seats. It never notifies the user. It gives one result to the chair and to nobody else.
- Every worker turn ends in one published result: complete, blocked, or failed. "No change, because ..." is complete. Blocked means the brief is unclear, two instructions disagree, a test and its spec disagree, or a named file or helper is missing. Then the worker publishes the question and its evidence. It does not ask in its pane, and it does not guess.
- At start, the worker confirms that the worktree, the paths, and the helpers in the brief exist, or it publishes blocked. It works only in the tree and the paths the brief names. When the sandbox denies a tool cache outside the workspace, it points the cache inside the workspace and never asks for a wider sandbox.
- A worker never reads `~/.claude/history.jsonl`, `~/.claude/sessions/`, `~/.claude/projects/`, `~/.claude-*`, `~/.codex-*`, or `~/.gemini`.

## Example
Initial state: a review seat must judge 6 units of a commit range.
Action: the brief says: you may write only `tmp/review-check/range-a-b-01/02-review-lens.md`; check each unit-rule pair your seat owns in `checklist.json`, with a quote and a proof; leave REF rows to the refs seat; read only the run folder and the files it names; stop when every owned pair has a row, then send one summary.
Result: the seat writes one file and one summary. `git status` in the repo shows no change that the seat made.

## Check (script)
- none: the brief and the seat's result live in the chair's run folder, and the commit range cannot show who wrote a file.

## Check (judge)
- Does each brief say what the worker may change, what it checks and how, what it leaves to others, its literal result path, and when it stops? Quote the five lines of one brief.
- Did a report-only seat change any file other than its result file? Quote the `git status` or the diff, or write "no change".
- Did each worker end with one result marked complete, blocked, or failed? Quote it.
