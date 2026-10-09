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

## When to flush the buffer (batch-ask), in priority order
1. Before a `git commit` this assistant is about to run, if the buffer is non-empty. Under
   `deliver`, flush once at task end instead; a direction-changing choice still stops the run.
2. Immediately after a **reversal** is confirmed (don't let it sit).
3. When the buffer reaches **~3 items**.
4. Before starting long-running work (so the "why" isn't lost to compaction).
5. On `/decisions`, or at session end.

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

## The capture loop (summary — full detail in `references/capture-loop.md`)
1. **Detect → persist** (silent, continuous): when a decision signal appears, append a
   flag `{decision, options seen, rationale-as-expressed, is_reversal}` to the WAL
   (`<root>/.git/decisions-buffer.json` — inside `.git`, so never tracked/committed).
   Persisting (not just holding in context) is what survives compaction and session end.
2. **Capture gate** (what reaches the buffer — *not* the record tier): capture a flag if
   the choice had a **real alternative**, OR it is a **reversal** of a prior decision.
   Skip routine coding choices. *Tier is decided later, at write time (step 6).*
3. **Consult prior decisions** (decision-aware retrieval): when a decision is being *made
   or changed* — not only at write time — search `docs/decisions/`
   for relevant prior records (see `capture-loop.md` §2.5 for the term rubric) and surface
   them. *Memory is only useful if consulted at decision time, not just written.*
4. **Contradiction check**: if a new decision conflicts with an *accepted* record but
   wasn't phrased as a reversal, flag it — "this contradicts ADR-0007: supersede it, or
   am I misreading?" Resolve as a reversal (step 7) or, if genuinely different scope, keep
   both and link via `related`.
5. **Batch-ask** at a breakpoint (see flush triggers above): present buffered flags as one
   numbered list — "I noticed these decisions — confirm which to record, and the one-line
   why." If a rationale lacks a rejected alternative or a downside, ask "why X over Y, and
   what does it cost?" Never write a record missing those — that's a "Fairy Tale" ADR.
6. **Write** each confirmed record (see `references/templates.md`), selecting the tier
   **now**: Tier-1 (cheap & local), Tier-2 (significant OR non-local), Tier-3 (costly AND
   non-local). *Every tier is a numbered file* — Tier-1 is a tiny stub (frontmatter +
   Y-statement). Set `related: [NNNN]` for records surfaced in step 3; run the
   pre-finalize checklist. Then **clear the flushed flags from the WAL**.
7. **Reversals**: a confirmed changed-our-mind item becomes a **new** record with
   `supersedes: OLD`. Rewrite the old record's frontmatter — `status: superseded` and
   `superseded-by: NEW` — leaving its body untouched. Never edit/delete the old body.
8. **Regenerate** `docs/decisions/README.md` from the files (newest first).

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
