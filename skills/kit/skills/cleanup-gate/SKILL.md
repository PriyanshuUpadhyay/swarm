---
name: cleanup-gate
description: "Run the full six-step cleanup pass only when the user explicitly asks for a cleanup pass — readability, simplification, dead-code removal, or triage of automated review comments — or when a substantial branch or pull-request diff reaches its delivery gate. This skill owns that activation rule: skip the pass for a routine diff. Do not use it to plan an unwritten change."
---

# Cleanup gate

One pass over the working tree, run BEFORE a substantial diff, patch, or pull request is presented.
It replaces the three after-the-fact passes the user otherwise pays for one at a time — readability
review, simplification, review-bot triage. The diff the user opens should already be the cleaned
diff, so the review budget goes to design instead of style churn.

**Activation.** The six-step pass runs only when the user explicitly asked for a cleanup,
readability, simplification, dead-code, or review-comment pass over work already written, or when a
substantial branch or pull-request diff has reached its delivery gate. A routine diff does not
activate this pass, and a `pair` task never does; its per-chunk `approve` is the gate. This skill
owns that decision — no other instruction file states it — and six
emitted steps over a two-file edit cost more than they return. When the pass is not activated, do
not emit its steps and do not announce that it was skipped.

**Entry contract.** Run this pass ONLY after scope and design are confirmed. Cleanup on unconfirmed
architecture is worse than no cleanup — a polished diff reads as finished and buys the wrong design
a pass through review. If any part of the scope or the design is unconfirmed, stop, name the
unconfirmed part, and get it confirmed before running step 1.

**Emission rule.** Each step below emits its result before the next step starts: either a list of
edits, one per line as `path:line — what changed — which rule`, or the literal word `none`. A step
that emitted nothing did not run. The six emissions are presented together, above the diff.

**Working set.** Only files this task changed. A file this task never touched is out of scope for
every step, including step 3 and step 4.

**Rules.** The global coding rules are authoritative for the smallest clear change, comments,
direct control flow and plain data, dead code, and unrelated cleanup. `references/rules.md` adds
only the rules the global file does not state, public contracts among them.

## The pass

Run in order. Do not reorder; step 1 decides what the later steps are allowed to touch.

### 1 — Scope delta

Map every changed file to a stated scope item. Emit the mapping.

Any file or hunk that no scope item requires is a ride-along: **report it, do not polish it.** For
each ride-along, offer two choices — revert it, or keep it with the one-line reason that promotes it
into scope — and wait for that answer before step 2 runs. Never clean a file you have just reported
as out of scope; cleaning it is what makes unrequested work look requested.

Emit: `<file> → <scope item>` per changed file, then the ride-along list, or `none`.

### 2 — Dead weight

Remove, within the working set: unreachable code, types and interfaces this task introduced and
nothing consumes, exports with no importer, wrapper layers with one caller and no behavior of their
own, and scaffolding left over from iteration.

Also inspect each dependency, configuration item, abstraction, service, retry, and compatibility path
introduced by the task. Remove it if it maps to no confirmed requirement and prevents no concrete
current failure. If removal would change the confirmed design, report the conflict and return to the
entry contract. Never use this test to remove required verification, security controls,
checks for irreversible actions, explicit user requirements, or established repository structure.

Emit: one line per removal with its reason, or `none`.

### 3 — Readability

Apply `references/rules.md` and the global coding rules. Collapse indirection that exists only
because an earlier iteration needed it.

Emit: one line per edit, or `none`.

### 4 — Comments and documentation

Apply the global comment rule and the public-contract rule in `references/rules.md` to the working
set: delete a comment that restates what the code already says, keep or add one only where a
constraint, trade-off, workaround, invariant, assumption, or edge case cannot be read off the code,
and document a public contract only where the signature does not carry its semantics, side effects,
or failure modes.

Emit two lists — added (each with the reason it is not obvious) and deleted — or `none`.

### 5 — Constant locality

Move every constant this task introduced into the single file that consumes it. A constant stays
shared only when two or more files read it. A constant that means nothing when read in isolation is
either misplaced or misnamed — fix which one it is.

Emit: one line per constant moved, renamed, or kept shared with the consumer count, or `none`.

### 6 — Test and review triage

Tests and fixtures: classify each test, fixture, or scratch file in the working set. Remove only the
temporary or out-of-scope ones — a throwaway spec written to prove the change works, a scratch
harness, a debugging snapshot, or a test for behavior no scope item covers. Keep every test the
change requires, including the regression test for a defect this change fixes and coverage for new
behavior.

Review bots: implement or refute each open automated review comment on this change, in one line
naming the `path:line` or the rule that settles it. Every comment gets a verdict here; do not
forward an untriaged comment to the user.

Emit: one line per test or fixture — `removed: <reason>` or `kept: <reason>` — and one line per
comment — `implemented: <what changed>` or `refuted: <reason>` — or `none`.

## Output

Present the six emissions first, then the diff. State plainly which steps found nothing. The user
should never be the first person to notice an item on this list.

## Red flags — stop and go back

| Signal | Action |
|---|---|
| The pass ran on a routine diff nobody asked to clean | It should not have activated. See **Activation**. |
| A rule attributed to the global file that is not in it | Fix the attribution, or move the rule into `references/rules.md`. |
| A step you ran "mentally" with nothing emitted | It did not run. Run it and emit. |
| A ride-along file you improved instead of reported | Revert the improvement; report the file. |
| Scope confirmed only by your own inference | Not confirmed. Return to the entry contract. |
| A comment added because the code looked bare | Delete it. Bare is the default. |
| A hand-rolled validator beside an existing schema library | Replace it. See `references/rules.md`. |
| An edit in a file this task never otherwise touched | Unrelated cleanup. Revert it. |
| A required regression test dropped from the diff | Restore it. Step 6 removes only temporary or out-of-scope tests. |
