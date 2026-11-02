---
name: worst-case-data
description: "Test a UI with real worst-case data from schema limits, fed at the data boundary, with Empty, One, and Huge cases at real width, 320 px, and 200 % zoom. Use when a UI shows user or backend data."
license: MIT
metadata:
  kind: pack
  owns: "worst-case data for a UI that shows user or backend data"
  serves: "build, review"
  needs: "a UI and the data that it shows"
  gives: "a field map with limits, a worst-case fixture, and a list of breaks with their fixes"
  conflicts: ""
  users: "implement, planner"
  source: "emilkowalski pack: skills/break-ui/SKILL.md and CATALOG.md (MIT), now blocks/worst-case-data/references/catalog.md; council 2026-10-30-skill-consolidation"
---

## Rules
- The kit root is two folders up from this file.
- Map each value that the UI shows: field, source, type, limit, and optional or not. Include counts, timestamps, badge text, labels from data, avatars, and the list length. Find the limit in the schema, the database column, the API type, or the form `maxLength`. If you find no limit, write "unbounded", which is a finding.
- Each worst-case value is one that a real user could make, or the real limit from the map. Never use random text such as `aaaa…`. Take values from `references/catalog.md` in this block's folder, and use `example.com` domains.
- Change the data, not the component. The worst case enters at the same boundary as the demo data: the fixture, the mock, the props, or the API stub.
- Put the different failures across the first rows of one fixture, as real data does. Also check Empty (no items), One (one item, and each count at exactly 1), and Huge (the real upper list length, 1,000 rows or more when the list has no pages).
- A switch between the demo data and the worst case is for development only, and it never ships.
- Look at each case at the component's real width, at 320 px, and at 200 % zoom. Shots follow `blocks/headless-shot/SKILL.md`.
- For each long string, choose wrap, end cut, middle cut, or clamp for that field. Never cut a number, an amount, or a date.
- List each break with its field, its value, what happens, and a fix at `file:line`. Mark it Broken, Ugly, or Fragile. Fix only the breaks that the step or the owner names, then check all the cases again, the demo data too.

## Example
Initial state: a member list shows a name, an email, and a count, with demo data "Jane Doe" and 12 members.
Action: the map finds `name` limited to 255 at `schema.ts:14` and `email` unbounded. The fixture puts `Aleksandra Wiśniewska-Kowalczyk` in row 1, a 70-character email in row 2, and `Jo` in row 3, with a count of 1. The 320 px shot shows the ••• menu pushed off the row.
Result: three breaks. The menu is Broken (fix: `min-width: 0` on the text column at `MemberRow.tsx:22`). "1 members" is Ugly. The email is Fragile because it has no limit.

## Check (script)
- none: the fixture, the shots, and the break list differ by repo, so the judge reads them.

## Check (judge)
- Does the field map give each shown value a source and a limit or "unbounded"? Quote the map.
- Does each worst-case value come from a real pattern or a real limit, and enter at the data boundary? Quote two values and the file that feeds them.
- Were Empty, One, and Huge checked at the real width, 320 px, and 200 % zoom? Quote the shot paths.
