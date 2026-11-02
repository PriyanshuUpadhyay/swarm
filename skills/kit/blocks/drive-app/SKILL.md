---
name: drive-app
description: "Drive the real app or browser one action at a time: expected result, fresh snapshot, one action, readback. Use when a proof or task needs real clicks, typing, or a native window."
license: MIT
metadata:
  kind: step
  owns: "how an agent drives a real app or browser"
  serves: "build, review"
  needs: "a running app or page and the result that an action must give"
  gives: "each action with its snapshot before and its readback after"
  conflicts: ""
  users: "implement, review, headless-shot, planner"
  source: "superset pack: skills/superset-computer/SKILL.md (snapshot, act, verify; failures; finish); anthropic-skills: chrome-browser/SKILL.md; council 2026-10-30-skill-consolidation"
---

## Rules
- Use the driver that the active host contract gives, such as a browser or desktop driver. If none is there, mark the step `unavailable <tool>`.
- Before each action, write the result that you expect, such as "Settings shows Dark mode selected".
- Take a fresh snapshot of the exact window or tab before each action. Pick the target by its role or label before its index, and by its index before pixels. A target from an old snapshot is stale after a repaint, a page change, or a dialog.
- Do one action, then take a new snapshot and check the expected result. A tool's success message proves only that the action was sent; `blocks/prove-it-works/SKILL.md` owns that rule.
- Send input in the background first. Use the foreground only when a fresh snapshot shows that the background action did not land. Retry only the step that failed.
- If the state is unclear, or an unknown dialog opens, stop and report what you see. Do not click through it.
- Work in your own session or tab. Close only what you opened, and leave windows, focus, tabs, and the clipboard as you found them.
- Capture only the target window, and crop out anything private before a shot leaves the machine.
- Sending a message, submitting a form, buying, deleting, or publishing follows the Safety rules of the active host. Logins, passwords, and MFA stay with the user.

## Example
Initial state: a fix must make the Export dialog keep the chosen format.
Action: expect "Format shows CSV". Snapshot the window, find the Format menu by its label, choose CSV, close and open the dialog, then snapshot again.
Result: the readback shows `Format: CSV`. The step records each action with its snapshot before and its readback after, so the claim is at step 5 of the proof ladder.

## Check (script)
- none: the snapshots and readbacks live in the step's record, which a script cannot read.

## Check (judge)
- Does each action have an expected result, a fresh snapshot before it, and a readback after it? Quote two actions.
- Did the run stop at an unclear state or unknown dialog instead of clicking through? Quote the stop, or write "none met".
