# Capture loop — full procedure

The hybrid model: **auto-flag silently during the session, batch-ask at a breakpoint.**
The write path is always user-confirmed.

## 1. Detect → persist to the WAL (continuous, silent)
The buffer is **persisted**, not just held in context — an in-context-only buffer does not
survive compaction, session end, or a model switch, which are exactly the long sessions
with the most decisions. On each detected signal, append an entry to the WAL:

- **Location:** `<repo-root>/.git/decisions-buffer.json` — inside `.git`, so it is never
  tracked or committed (no `.gitignore` edit needed). Use Read/Write tools (allowlisted).
- **Entry:** `{ summary, options_seen, rationale_as_expressed, is_reversal, supersedes? }`.
- Append, don't interrupt: capture the **rationale while it is still in context** (the gap
  diff/CI tools provably cannot fill). The user is asked only at the batch-ask (§3).
- On skill activation, **read the WAL first** — it may hold flags from a prior session
  that were never flushed.

Two trigger classes:

**Explicit → high-confidence.**
- "let's go with X" · "we'll use X instead of Y" · "decided on X" · "going with X"
- "record/log this (decision)" · "write an ADR for this"
- **Reversals:** "actually, switch to…" · "scrap that, we'll…" · "changed my mind" ·
  "revert to…" · "let's not do X after all"

**Implicit → lower-confidence (surface at batch-ask, never auto-write).**
- framework / library / database choice
- schema or API design (REST vs GraphQL, table shape, event contract)
- auth / authz strategy
- monolith vs service split, module boundaries
- build / deploy / CI tooling

## 2. Capture gate (what reaches the WAL — separate from record tier)
Append a flag if the choice **had a real alternative**, OR it is a **reversal** of a prior
decision (reversals always qualify). Skip routine coding choices (variable names, local
refactors, obvious one-way calls).

This gate is deliberately *broad* — it decides what is worth remembering, not how much
ceremony to give it. The **tier** (how much to write) is chosen later, at write time (§5),
from the cost/locality of the decision. Conflating the two would make the cheaper tiers
unreachable. *If every decision is "architectural", no decision is — but cheap decisions
still deserve a one-line Tier-1 stub.*

## 2.5 Consult prior decisions (decision-aware retrieval)
*Memory is worthless unless consulted at decision time — not just written.* (Neo4j
context-graphs talk: "what did you do before in this situation?")

Trigger this **whenever a decision is being made or changed** (not only at write time),
and on `/decisions check "<topic>"`. Use a **deterministic term rubric** so it's
repeatable, not vibes:
1. Read `docs/decisions/README.md` first — for a small ADR set the index is the most
   reliable scan, and it's the entry point. Use the **Grep tool** (allowlisted; not bash
   `grep`) over `docs/decisions/` for the long tail.
2. Search these term classes drawn from the pending decision:
   - the **technology / library / product** names (and obvious aliases — "auth" *and*
     "OAuth"/"login"/"session"; "db" *and* the engine names),
   - the **component / module / service** name,
   - **touched file paths**,
   - domain terms: **API · schema · auth · storage · cache · protocol · deploy · boundary**,
   - the **rejected alternative**'s name (catches "we already ruled this out").
3. Read the accepted hits (don't act on titles alone). Surface them inline with status:
   > "ADR-0007 chose Postgres for transactions (accepted). ADR-0003 set the auth strategy
   > (superseded by 0011). Both look relevant to what you're deciding."
4. Use them as precedent — stay consistent unless there's a reason to overrule (a
   contradiction, §2.6).
5. Record any surfaced records as `related: [NNNN]` on the new decision.

## 2.6 Contradiction check
**Flag a contradiction** when the new decision, against an *accepted* record, does any of:
replaces the **same** technology / storage / protocol / schema / auth scheme / service
boundary / deployment target — or **reverses a previously rejected option**. (A different
component or a strictly additive choice is *not* a contradiction.)

If it contradicts but the user didn't phrase it as a reversal, don't silently write a
parallel record — surface it:
> "This contradicts ADR-0007 (accepted: Postgres for transactions). Supersede it, or am
> I misreading the scope?"

Resolve one of two ways:
- **It is an override** → treat as a reversal (§6): new record `supersedes` the old.
- **Different scope, both valid** → keep both; link with `related: [NNNN]` and note the
  scope distinction in Context. Never leave two accepted records in unflagged conflict.

## 3. Batch-ask (flush triggers — flush early and often)
The WAL guards against loss, but the sooner the user confirms the "why", the better.
Flush, in priority order:
1. **Before a `git commit` this assistant runs**, if the WAL is non-empty (highest-signal
   moment). Behavioral, not an installed hook. *Known gap:* commits made outside this
   assistant (manual terminal, GUI, another agent) won't trigger this — the WAL then
   carries the flags to the next `/decisions` or next session.
2. **Right after a reversal is confirmed** — don't let it sit.
3. **WAL reaches ~3 items.**
4. **Before starting long-running work** (so the "why" isn't lost to compaction).
5. **On `/decisions`, or at session end.**

Present the WAL as a single numbered list. Example:
```
I noticed these decisions. Which should I record? (and the one-line why)

  1. Postgres over Mongo   why captured: "need transactions"
  2. pino over winston     why captured: "perf"
  3. (reversal) drop Redis layer, was ADR-0001   why: ?

Reply e.g. "1 and 3" / "all" / "skip 2" — and fill any missing why.
```
If a rationale lacks a **rejected alternative** or a **downside**, ask before writing —
a record missing either is a "Fairy Tale":
> "For #1 (Postgres over Mongo): what did we reject, and what does choosing Postgres cost?"

## 4. Choose the tier (at write time)
Per the gate in `templates.md`:
- cheap to reverse **AND** localized → **Tier-1** stub (numbered file, frontmatter + Y-statement)
- significant **OR** non-localized → **Tier-2** MADR-minimal
- costly-to-reverse **AND** non-localized → **Tier-3** MADR-full

## 5. Write
1. Resolve the repo root + next number via the folder-discovery procedure in SKILL.md
   (`git rev-parse` + the Glob tool; empty → `0001`).
2. Fill the chosen tier's template from `references/templates.md`. Set `status: accepted`,
   `date` = today, `deciders` if known, `related: [NNNN]` for records surfaced in §2.5,
   and `informed-by` for the sources (files/docs/data) that drove the choice.
3. Run the **pre-finalize checklist**. Fix failures or re-elicit.
4. Write `<root>/docs/decisions/NNNN-title-with-dashes.md` (every tier is a file).
5. **Clear the flushed flags from the WAL** so they aren't re-offered next session.

## 6. Reversals (auto bidirectional supersede — from adr-tools)
When a confirmed item reverses a recorded decision OLD:
1. Create the **new** record NNNN normally, with frontmatter `supersedes: OLD` and a
   `## Links` line "Supersedes ADR-OLD".
2. **Rewrite the old record's frontmatter** (the only permitted edit to an accepted
   record): `status: superseded` and `superseded-by: NNNN`. Add a Links line "Superseded
   by ADR-NNNN". Leave its body untouched.
3. Both records now point at each other. The old rationale survives intact.

`/decisions supersede NNNN` runs this flow directly.

## 7. Regenerate the index
After any write, rebuild `docs/decisions/README.md` — **fully from the record files**
(every tier is a file, so nothing lives only in the index):
- Read every `NNNN-*.md`, parse frontmatter (`status`, `date`, `supersedes`,
  `superseded-by`, `related`; tier from structure; title from the `# NNNN.` line).
- Emit the table newest-first (by `date`, then number) with the `Refs` column derived
  from frontmatter (`↑NNNN` supersedes, `~NNNN` superseded-by, `→NNNN` related) — the
  lightweight relationship "graph". Use the format in `templates.md`.
- Overwrite the file — it is fully derived, never merged by hand.
