---
name: done-when-evidence
description: "Map each done-when of the frame to its evidence, or mark it unverified, and in research give each source type a result. Use when you close a change or a research report."
license: MIT
metadata:
  kind: rule
  owns: "the map from each done-when claim to its evidence"
  serves: "close"
  needs: "the frame's done-when"
  gives: "one evidence row per claim"
  conflicts: ""
  users: "implement, research"
  source: "presets/implement/PRESET.md (step table, 07-close row); references/plan-layout.md:31-33 (Verification); blocks/prove-it-works/SKILL.md (the ladder: below step 4 is unproven)"
---

## Rules
- This block answers which proof meets each claim. For proof strength, see `blocks/prove-it-works/SKILL.md`; for source agreement, see `blocks/cite-sources/SKILL.md`.
- The close step's file lists every done-when of the frame, and in a plan each `Done when:` claim bullet too, each with its evidence.
- Use a table or a numbered list: requirement, check, evidence. Every requirement and every accepted constraint has a row.
- Each row's check is the one that the build step's file names for it, in a todo's evidence or its `## Result`. In implement, review judges that each done-when has one there, before this table exists.
- Evidence reads "not run" until you get it. A row with no proof reads "unverified" and says why.
- Keep a partial check apart from a completed proof.
- In research, the table also has one row for each source type: local sources, surveys, origin papers, recent work, and off-path ideas. Each row holds its result with a link, or `not applicable: <reason>`. A link check does not prove coverage, because it shows only that the links open.
- When a source type has a gap, say whether the gap blocks the answer. Write `blocks: <which conclusion>` or `does not block: <why>`.

## Example
Initial state: `01-frame.md` has three done-when lines, and `05-build.md` names a check for each.
Action: write in `07-close.md`:
| Done-when | Check | Evidence |
|---|---|---|
| Hero states the tool in 20 words | word count script | `checks/hero.mjs -> PASS 18 words` |
| Diagram 2 matches `docs/architecture.md` | diagram-check table | `06-review.md` rows 1-7, all cited |
| Page looks right on a phone | screenshot at 390 px | `tmp/shots/hero-390.png`; taste awaiting the user's review |
Result: two rows proved, one partial row named as partial.

## Check (script)
- none: the close table lives only in the step file, which holds this block's own Rules; in implement, `take 07-close` already refuses a run without APPROVE on HEAD.

## Check (judge)
- Does each done-when line of the frame and each accepted constraint have a check, planned or run, named in the build step's file (a todo's evidence or its `## Result`)? Quote each done-when and its check.
- Does any done-when have no check named there? Quote it, or write "none".
- Is each named check a command, a test, or a screenshot path that a close table can cite? Quote each check.
- In research, does each source type have a result with a link or `not applicable:` with a reason, and does each gap say whether it blocks the answer? Quote the rows.
- Is any planned or partial check in the build step's file written as a completed proof? Quote the line, or write "none".
