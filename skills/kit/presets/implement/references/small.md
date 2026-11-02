# implement route: small

Example. The owner asks for a `--json` flag on one CLI command. It needs no new decision, no
contract or stored-data change, and no security choice, so the agent takes the small route. It
writes a failing test, adds 35 lines in 2 files, and the test passes. `size_gate.py` prints
`small: 2 files, 35 lines`. One fresh review seat runs the review check, which prints `APPROVE`.
The agent commits and replies with the test output and the verdict line.

Rule: a small change gets one agent, real tests, and one independent review, with no plan page and no step files.

## Intake gate

Take the small route only when all of these hold. Otherwise run the `implement` preset in full.

- The change needs no new decision: no fork that the owner or a contract must settle.
- It changes no public interface, stored data, wire format, permission, or secret.
- The owner did not ask for a plan, a review page, or pair mode.
- It fits in about 3 files and 100 changed lines, not counting test files.

## Steps

1. Write the request, the done-when, and `Base: <commit>` in the reply before the first edit.
2. Write a failing test, or name the existing check that will prove the change, by
   `blocks/prove-it-works/SKILL.md`. Make the change. Run the test and paste its output.
3. Run `python3 -I <kit root>/presets/implement/scripts/size_gate.py <base>` from the repo root.
   On `raise: <reason>`, stop the small route, tell the owner the reason, and run the `implement`
   preset in full on the same change. Its review covers the same `<base>..HEAD`.
4. Commit. Run `python3 <kit root>/presets/review/scripts/review_check.py start <base>..HEAD`, and
   send the run's seat map to one fresh seat that the `run-workers` block starts. One seat writes
   every named seat file, as the Seats section of the `review` preset's `references/check.md`
   allows. The agent that wrote the change never fills the rows.
5. Run `verdict`. On `REQUEST CHANGES`, fix each `fix` row and review the new range once more. On
   `NEEDS DISCUSSION`, bring the asks to the owner. A second `REQUEST CHANGES` goes to the owner.
   On `INCOMPLETE`, run `review_check.py build <run> --run` when `01-units.md` has a Build line,
   have the seat fill each missing row, and run `verdict` again.
6. Reply with the test output, the `size_gate.py` line, and line 3 of `03-verdict.md`.
