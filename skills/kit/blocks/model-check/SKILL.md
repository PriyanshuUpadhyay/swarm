---
name: model-check
description: "Model-check a race or retry finding with a small TLA+ model and everystate, and put the trace in the review row's proof. Use when a review row's proof is an order of steps across awaits, retries, callbacks, alarms, or locks."
license: MIT
metadata:
  kind: step
  owns: "when and how a review seat models a concurrency finding, and what its result proves"
  serves: "review"
  needs: "a review unit whose row names an order of steps"
  gives: "a model, its checker result at head (and base), a mutant result, and a proof cell"
  conflicts: ""
  users: "code-review"
  source: "everystate pack: skills/formal-verification/SKILL.md (MIT, mattzcarey/everystate 0.4.1); trial 2026-11-19, 8 PRs in 2 rounds, blind graded: model-backed one-arm findings 3 true, 1 unclear, 0 false; plain one-arm findings 1 true, 2 unclear, 1 false"
---

## Rules
- Use it for a unit whose `fix` or `ask` depends on the order of steps: a read and a write split by an `await`, two handlers or callbacks on one state, a retry or re-delivery, an alarm or Workflow re-entry, or a lock's scope. This is the `L-3`, `L-4`, `L-12`, `CF-12`, `SRV-11`..`SRV-13`, and `SQL-8` kind of row. Do not use it for capacity, cost, latency budgets, or one-input logic; read the code for those.
- Run `everystate -version`. Use the binary on PATH; a seat never installs it. When it is missing, write `limit: model-check not run, everystate missing` in the row's proof and keep the row as reading alone makes it.
- Write the model in the review run folder, `<run>/models/<unit>/`, never in the repo. Write one variable per piece of state that matters and one action per step that is indivisible in the code. A read and a write split by an `await` are two actions. Add the failures the code can meet: a lost reply, a timeout, a crash, a retry. Put the file and line of the code on each action as a comment. Bound it in the `.cfg` with 2 or 3 actors and 2 or 3 retries.
- State the promise as an `INVARIANT`, or as a `PROPERTY` with fairness for "eventually". Run `everystate -json <Spec>.tla` at the head. When the change claims to fix an order, also model the merge base: base fails and head passes is the evidence for that claim.
- Check every step of a failing trace against the code. When the model allows a step the code cannot take, fix the model and run it again; the first trace is not a finding.
- For each model that passes, break it the way the code could break (remove the guard, split the atomic step), run it, and record that it fails. A model that never failed proves nothing.
- Put the model path, the exit code, and the trace as a short story in the code's words in the row's `proof` cell. A trace shows that the order is possible in the model, not that a caller takes it. So the row kind and the way an `ask` becomes a `fix` stay as `skills/code-review/references/check.md` sets them.
- Report what the model leaves out (the steps merged into one action, the bounds, the platform behavior assumed) in the seat's result.

## Example
Initial state: a review unit has two reconcilers that each read a batch, await an RPC, then claim it. The seat writes an `ask` for a race on the claim.
Action: the seat writes `models/reconcile/Batch.tla` with actions `Read(r)`, `Claim(r)`, and `Fail(r)`, two reconcilers, and the invariant `NoThrowOnFailed`. At head it exits 12 with the trace Read(A), Read(B), Claim(A), Fail(A), Claim(B). The seat checks each step against the code, and the mutant "claim returns a failed batch" exits 0.
Result: the row stays an `ask`, and its proof names the model, `exit 12`, and that story. The author writes a failing test from it.

## Check (script)
- none: the models and their output live in the review run folder, which differs for each run.

## Check (judge)
- Does each modeled row's proof name the model file, the exit code, and a trace told in the code's words? Quote one proof.
- For each model that passes, is there a mutant that fails? Quote the mutation and its exit code, or write "no passing model".
- Was any row moved from `ask` to `fix` on a trace alone, with no failing test? Quote it, or write "none".
