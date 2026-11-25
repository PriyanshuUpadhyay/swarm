# Whole-change seat

The `whole` seat reads the change as one piece, the way a human reviewer does. The other seats
answer one rule on one unit, so no one of them asks whether the change does what it says, or
whether a value still holds after it leaves the file. This seat owns no rule ID. It runs on
`review.deep`, in every check run, and writes `02-review-whole.md`.

Example. A change stubs old tool results and stores a call ID in the stub. The seat writes
`Intent: cut prompt size on a cold turn; a stubbed result is read back by its call ID`. Then it
follows the call ID to the reader and finds that the reader loads only the messages after the
summary, so an older call ID gets `not_found`. It writes that as an `ask` with the two lines.

## Order of work

1. Read `intent.md` in the run folder, then `diff.patch`, then the full files in `head/`. Do not
   read the other seats' files.
2. Write what the change does and why, before any finding. Line 4 of the file is
   `Intent: <one or two sentences>`. Below it, list the main parts of the change, one line each.
   Where the code does less or more than `intent.md` says, name the difference.
3. Attack the intent. For each part, ask what breaks it:
   - data that crosses a file, a turn, a deploy, or a stored row: follow each value to its reader;
   - the cost and safety limits the change sets: what input passes them, and what happens then;
   - what the old code did that the new code no longer does;
   - a call to an outside service: read its source or its doc. A comment in the code, a doc in the
     repo, or an earlier answer is not proof of what the service does;
   - a part that the intent does not need.
4. For each row of `answers-before.md` that starts with `fix:`, read the code that fixed it. Ask if
   the fix is true and if it is the simplest one. A fix that added a constant, a wrapper, or a
   comment in place of a behavior change gets a row.

## Rows

Write the rows as `check.md` says, with rule `-` and the unit that holds the line. Put the worst
finding first. A defect you ran or traced to a failing input is a `fix`. A defect you could not
confirm is an `ask` that starts with `potential issue, not confirmed:` and names the input. With no
finding, the file holds the `Intent:` line, the parts, and no row.
