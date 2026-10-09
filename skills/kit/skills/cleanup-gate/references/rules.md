# Standing cleanup rules

These are the user's rules that the global coding rules do not already state. Each was a
correction the user had to make after reading a diff. The gate exists so they are applied before the
diff is shown.

The global file already owns the smallest clear change, the comment rule, direct control flow and
plain data, dead-code removal, and the ban on unrelated cleanup. Do not restate those here; apply
them from the global file. It says nothing about public contracts, tests, boilerplate, constants, or
schema libraries — those are the rules below.

## 1 — Only temporary or out-of-scope tests and fixtures leave the diff

Remove a test, fixture, or scratch file from the change only when it is temporary or out of scope:
a throwaway spec written to prove the change works, a scratch harness, a snapshot of debugging
state, or a test for behavior no scope item covers. Delete those, or move them outside the staged
set, before the change is presented.

Keep every test the change actually requires — a regression test for a fixed defect, a test the
repository's convention or the user asked for, and a test that covers new behavior this change
introduces. Those belong in the diff. Check which kind each test file is before presenting, not
after the user asks.

## 2 — No unnecessary boilerplate and no unused types

An entry point, wrapper, interface, or type that exists without a consumer is removed, not kept for
a future that has not been requested. The `minimize-reader-load` skill decides when a layer stays
and when it is inlined.

## 3 — Constants local to their consumer

A constant used by exactly one file lives in that file. It is shared only when two or more files
read it. A constant that carries no meaning when read in isolation is misplaced, misnamed, or
unnecessary — decide which and fix it.

## 4 — No hand-rolled validation where a schema library exists

If the repository already depends on a schema validation library (`zod` or the local equivalent),
use it. Do not write a parallel validation helper, a manual field check, or a bespoke type guard
next to it.

## 5 — Public contracts documented only where the signature does not carry them

Document a public contract only when the signature does not convey its semantics, constraints, side
effects, or failure modes. The global comment rule covers ordinary comments; this rule covers the
documented surface other code depends on.
