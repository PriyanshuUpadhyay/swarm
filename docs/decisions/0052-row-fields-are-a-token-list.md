---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0049", "0053", "0054", "0057"]
informed-by:
  - "Owner verdict on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-4 changed to (b) with all tiers, S-14, G-24, T-25"
  - "Herdr and Warp rows are token lists the user edits (report ~/.claude/reports/2026-10-06-swarm-sidebar-customization.md)"
---

# 0052. Each row level shows an ordered list of fields that the owner picks

## Context and Problem Statement

A workspace row showed a fixed detail line: path qualifier, project, branch and "N chats". The
owner wants other data that the app already has or can read, such as model, dirty count, PR, CI
and cost, and wants it on chat rows and tabs as well.

## Considered Options

- A fixed set chosen in code.
- An ordered token list (field names) for each row level: project, workspace, chat and tab.
- A free template string such as `{title} — {branch}`.

## Decision Outcome

Chosen: a token list for each level, stored with the owner's other choices (ADR 0057). Any field
may be picked. Cheap fields come from data the app holds: status, provider, title, branch, last
activity, child count, waiting question and unread. Medium fields read on a slower timer: dirty
count, model, effort and step progress. PR and CI come from a cached, rate-limited `gh` call. Cost
comes from the agent row (ADR 0053). A tab shows an unread dot for a finished turn the owner has
not seen, from a last-seen time for each chat. Last activity is the newest of the last bus message
and the chair log's write time. Rejected: a template string, because a typo fails silently.

### Consequences

- Good: the owner sees the data that matters to them without a code change.
- Bad: slow fields need caching and timers, and a field list that picks all of them costs network
  calls and git reads.
