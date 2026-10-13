---
name: link-check
description: "Before a report is done, open every link and find the claim's words on the page; mark dead or unsupporting links and drop claims that rest only on them. Use when a report or merged result cites web links."
license: MIT
metadata:
  kind: rule
  owns: "the check that every cited link opens and supports its claim"
  serves: "report, merge"
  needs: "a report with links"
  gives: "a checked link list with dead and unsupported links marked"
  conflicts: ""
  users: "research, web-search"
  source: "skills/research/SKILL.md (05-report link check, moved here 2026-10-13); skills/web-search/SKILL.md (Chair synthesis step 2)"
---

## Rules
- Open every link before the report is done. An HTTP status shows only that the page opens, so also find the claim's number or wording on the page you cite.
- Mark a link that does not open "(link dead)", and one that does not support its claim "(does not say this)". Drop a claim that rests on such a link alone.
- When only a seat could open a page, mark the link "(seat-read only)".
- Write the result of the check in the step file: links checked, dead, and unsupported.

## Example
Initial state: a report cites six pages.
Action: open each one. One returns 404. Another opens but never states the number the claim cites.
Result: four links stay, one reads "(link dead)", the claim that rested only on the sixth page is dropped, and the step file says "6 checked, 1 dead, 1 unsupported".

## Check (script)
- none: the report and its links live outside the commit range.

## Check (judge)
- Does the step file give the counts of links checked, dead, and unsupported? Quote it.
- Does any claim rest only on a link marked dead or unsupported? Quote it, or write "none".
