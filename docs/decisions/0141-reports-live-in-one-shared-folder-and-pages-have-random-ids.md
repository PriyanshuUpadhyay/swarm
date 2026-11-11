---
status: accepted
date: 2026-11-11
deciders: [user]
amends: "0101"
related: ["0101"]
informed-by:
  - 'owner, 2026-11-11: reports must not live under a Claude path; move them to a shared place under .swarm; give each published report a UUID; when hosting, expose only the named ID; the home path must not list the pages'
  - 'owner, 2026-11-11: accepted every recommended default; the owner hosts the shelf by hand, so swarm adds no tunnel or Host option'
---

# 0141. Reports live in one shared folder and pages have random IDs

## Context and Problem Statement

The kit sent Claude reports to `~/.claude/reports/`, and Codex and AGY reports to a worktree's
`tmp/reports/`, so a report from one agent CLI was not visible to the others and a worktree removal
deleted it. `swarm pages` named a page `<YYYY-MM-DD>-<slug>`, which a person can guess, and its home
page listed every page. The owner wants to put one page on the network by hand without exposing the
rest.

## Decision Outcome

- Reports go to `~/.swarm/reports/` for every runtime. `~/.claude/reports/` becomes a symlink to it,
  so links in older ADRs still open. `references/run-folder.md` owns the rule.
- `swarm pages publish` copies a page to `.swarm/pages/<id>/`, where `<id>` is a random UUID v4
  (122 random bits; v7 starts with a timestamp, so it is not used). A later publish of the same
  source folder keeps its ID. The slug, date and source go in `<id>/.page.json`, which the hub
  never serves because it refuses dot entries.
- The hub serves only paths whose first part is a page ID, and answers 404 at `/`. The page list is
  in `swarm pages` only.
- `swarm pages --foreground --id <id>...` serves only the named pages, so the owner can put that one
  process behind a proxy of their choice.
- A page folder from before this ADR gets a random ID on the next `swarm pages` call, so its old
  URL stops working.

### Consequences

- Good: a report from any agent CLI is in one place, and a shared link does not reveal other pages.
- Good: a republished page keeps its URL across days.
- Bad: the hub still accepts only a loopback Host, so a proxy must send `Host: localhost:<port>`.
- Bad: Swarm UI has no reports view yet, so it cannot publish a report; that is a later change.

## Links

- Amends: 0101.
