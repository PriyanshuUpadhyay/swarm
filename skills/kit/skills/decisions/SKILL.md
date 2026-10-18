---
name: decisions
description: Record project decisions as committed in-repo ADRs. Use when a consequential choice or reversal is expressed, such as "let's go with X", "use X instead of Y", "scrap that", or "changed my mind", or on /decisions and "write an ADR". Also flush the decision buffer before an assistant-run git commit. Skips routine coding choices; writes are always user-confirmed.
---

# decisions — Lifecycle Decision Records

Capture *what was decided, why, what was rejected, and what changed* as committed,
in-repo ADRs (Architecture Decision Records). Successor in spirit to "Context Craft".

**Capture is hybrid:** auto-flag decision signals silently during the session into a
**persisted decision buffer** (a gitignored WAL at `<repo>/.git/decisions-buffer.json`),
then **batch-ask** the user to confirm + supply the "why" at a breakpoint. The write path
is *always* user-confirmed — this skill never journals unprompted.

**Reliability is best-effort, not enforced** (zero-install by design — no git hook). The
WAL makes the buffer survive context compaction, session end, and model switches; flush
early and often (see triggers below) to minimize loss. *Limitation:* commits made outside
this assistant (manual terminal, GUI, another agent) won't trigger a flush — capture then
relies on the next `/decisions` or the WAL being picked up next session.

Templates + frontmatter + anti-pattern checklist: `references/templates.md`. Full capture
procedure: `references/capture-loop.md`.

## When to Trigger
- An explicit decision is expressed: "let's go with X", "we'll use X instead of Y",
  "decided on X", "record/log this decision", `/decisions`.
- A **reversal**: "actually, switch to…", "scrap that", "changed my mind", "revert to…"
  — reversals are first-class and always qualify.
- A consequential choice surfaces (framework/library/db, schema/API design, auth strategy,
  service boundaries, build/deploy tooling).

## When to flush the buffer

Apply the flush schedule in `references/capture-loop.md` section 3, including its `deliver` scope.

## When NOT to Trigger
- Routine implementation detail, variable renames, local refactors — noise, not decisions.
- A choice with no real alternative and no cost to reversing it.
- Unscoped exploration — that's `superpowers:brainstorming`, not a decision yet.

## Verbs (dispatch on the normalized request)

Normalize the request into a verb plus its operand before doing anything. Take them from the
arguments supplied with the invocation; when the invocation supplies none, take them from the
user's request text. No verb and no operand means `flush`. A verb that needs an operand
(`new`, `supersede`, `check`) fails closed: with no operand in either place, stop and ask —
never invent a title, a record number, or a topic.

| Request | Action |
|---|---|
| `/decisions` (no args) | Flush the decision buffer: batch-ask, then write confirmed records |
| `/decisions new "title"` | Manually scaffold one record (asks tier + why) |
| `/decisions supersede NNNN` | Record a reversal that supersedes NNNN (auto bidirectional link) |
| `/decisions check "<topic>"` | Decision-aware retrieval: surface prior records relevant to a topic/change before deciding |
| `/decisions index` | Regenerate `docs/decisions/README.md` from the record files |
| `/decisions status` | Show buffered (unwritten) flags + recent records |

## Folder discovery
1. Resolve the repo root (one bash call): `git rev-parse --show-toplevel` — if it fails
   (not a git repo), fall back to `pwd`. `DECISIONS_DIR = <root>/docs/decisions`.
2. Ensure it exists: `mkdir -p "<root>/docs/decisions"`.
3. Compute the next number by globbing
   `docs/decisions/[0-9][0-9][0-9][0-9]-*.md`, take the numeric 4-digit prefix of each
   match, find the max, add 1. Empty result → next is `0001`. (Globbing avoids the
   lexical-sort and empty-dir pitfalls of `ls`/`find`.)
- New file: `<root>/docs/decisions/NNNN-title-with-dashes.md` (lowercase, present-tense).
- **Worktree note:** `git rev-parse --show-toplevel` returns the *current worktree's*
  root, so records written from a linked worktree land in that worktree's tree — numbering
  can fragment across worktrees (accepted v1 tradeoff; resolve at merge).

## The capture loop

Read `references/capture-loop.md` in full. It owns the buffer entry shape, capture gate,
prior-decision lookup, flush schedule, confirmation, writing, reversal procedure, and index rebuild.
Apply the storage contract below to every write.

## Storage rules (load-bearing — from Nygard + MADR + adr-tools)
- **One decision per file — every tier**, including Tier-1 stubs. Numbers are monotonic
  and **never reused**. (This makes the index *fully* re-derivable from files.)
- **Body-immutable.** Reverse a decision by superseding it, never by editing/deleting its
  body — the abandoned rationale is the most valuable thing to preserve. The *only*
  permitted edit to an accepted record is its status/`superseded-by` frontmatter on a
  reciprocal supersede link.
- **In-repo, committed.** Records are team-shared history, not machine-local memory. (The
  WAL is the sole exception — it lives in `.git/` and is never committed.)
- **Index is fully derived** from the record files; regenerate after every write, never
  hand-edit.

## Bootstrapping an existing repo
Offer to paste the pointer from `references/core-instructions-snippet.md` into the repo's
always-loaded instruction file so future sessions read prior decisions before proposing
changes. Confirm the target file before editing it. Then offer a one-time backfill: scan recent
history (`git log --oneline`) for landed decisions and batch-ask which to record.
