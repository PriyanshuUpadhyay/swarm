---
name: minimize-reader-load
description: Owner rule for when code gets its own function, file, or module, and when it stays inline. Apply before a chunk or a review adds, splits out, or keeps a helper, wrapper, layer, or module, or when code is hard to trace. Read when the pair or review-check skill points here.
disable-model-invocation: true
---

# Minimize reader load

Modularity must reduce the number of concepts a reader holds in their head. Fewer lines per
function is not the goal. Track two costs, and guard both:

1. Layers to trace. How many jumps sit between the question and the answer.
2. State to hold. How much hidden or mutable context the reader must keep in their head.

Keep code inline by default. Give it its own function, file, or module only when at least one of
these is true:

1. It is reused. Two or more current callers use it.
2. It has a clear domain name. The name is a concept of the domain, not a step such as
   `process_part_2` or `handle_data`.
3. It hides substantial complexity. A caller can use it without reading its body.
4. It has meaningful behavior that needs its own test.
5. It sets a real architectural boundary, such as a network, device, or storage edge, a trust
   boundary, or a public API.

"This function or file is getting long" alone is not a reason. The test also works in reverse. A
helper with one caller that meets no condition goes back inline, and a pass-through layer that
repeats the same methods and arguments collapses.

Shrink state the same way. Prefer a returned value over a mutation, a local over a field, a field
over module state, and module state over a global. Derive a value instead of syncing two copies.
Name an invariant once, at the boundary, not in every consumer.

Example. A 60-line `import_orders` reads a CSV, checks each row, and writes the rows. A change
splits it into `_read_rows`, `_check_row`, and `_write_rows`, each with one caller and straight-line
code. No condition is true, so the reader now tracks four names and three jumps for the same flow,
and the split is a defect. If the API handler also calls `_check_row`, condition 1 is true and that
one extraction stays.

The test. A new reader can answer "where does X come from?" and "what can change X?" in under 30
seconds. If not, cut layers or cut state.
