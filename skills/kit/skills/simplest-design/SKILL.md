---
name: simplest-design
description: "Propose the design with the fewest parts that meets the stated needs. Trace each part to a need, reuse what the code already gives, and cut the rest. Use when you propose a design, plan, or approach before code."
---

# Simplest design

Example. Initial state: the user asks to keep a user's entitlements in sync between two services.
Action: the first draft has a push and pull sync, a cache of the whole user object, and a migration.
The parts table shows that no need asks for a push, that only the entitlements are read, and that
the stored shape is in no release yet.
Result: the design caches only the entitlements and reads them on a miss, with no migration. The
reply lists the three cut parts, one line each.

In five real runs the owner removed parts from a design that no need asked for: a cache key that
the array order already gave, a `totalUsage` fallback for a value that `usage` already held, a
wrapper on every call where the shared setup could hold it, a push and pull sync, and a migration
for data that was not in production.

## Steps

1. **Needs.** Write the needs as a numbered list, in the user's words, with each constraint the
   repository states, such as a docstring, an ADR, or a test that fixes a rule. Add nothing the
   user did not ask for.
2. **What is already there.** For each need, search the code for what already gives it, such as a
   value, an output line, a shared setup, a constant list, or a helper. Quote the `file:line`.
3. **Parts.** Draft the design, then write it as a table with one row per part: the part, the need
   number it serves, and why the existing code does not already give it.
4. **Cut.** Remove each part that fails one of these tests:
   - It serves no need number. A part for a later need becomes a one-line question, not a part.
   - It repeats a value, a line, or a step that step 2 found. Use the existing one.
   - It keeps an old name, shape, or path working, such as a fallback, an alias, or a migration.
     Keep it only when the old shape has shipped. Check this with `git tag --contains <commit>`
     or the deployed data, and quote the result.
   - It is a flag, a config file, or an environment variable. Keep it only when a need names who
     sets the value. A repository rule that fixes the value beats it.
5. **Walk one input.** A smaller design must still be correct. Pick one input where a new part
   meets code that already handles part of the same input, for example an order that both a new
   coupon rule and the existing sale price reduce. Follow it through the real code and the design, write
   the numbers or the output at each step, and fix the design when the input is handled twice or
   not at all. Make each new pattern match only what the needs name.
6. **Reply.** Show the design, the parts table, the walked input, and a `Cut` list with one line
   per cut part and the test it failed. Put the questions last, one line each.

## Rules

- Each part in the design traces to a need number. A part with no number is cut or asked.
- Reuse beats new code. Name the existing `file:line` before you add a part beside it.
- A compatibility part needs proof that the old shape shipped.
- A design is shown only with one walked input where a new part meets existing code.
- This skill shapes the design only. The calling workflow keeps its approval steps.
