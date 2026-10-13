---
name: page-comments
description: "Carry the owner's comments on a built page through rounds of fixes and checks until they approve it. Use when the owner reviews a rendered page, a walkthrough, or a demo before it is done."
license: MIT
metadata:
  kind: step
  owns: "the loop of comment round, fix, and check for a built page"
  serves: "view and talk"
  needs: "a built page, its checks, and the block that built it"
  gives: "the approved page, and an answered round file per round"
  conflicts: ""
  users: "planner"
  source: "docs/blocks.md (view and talk: a loop with a cap, ended by the owner); blocks/comment-round/SKILL.md; blocks/plan-chart/SKILL.md (a plan has no loop, so the loop lives in this step); docs/council 2026-10-13 block rules (D5, owner decision)"
---

## Rules
- The kit root is two folders up from this file.
- Run each round with the `comment-round` block on the page's files, with `<run>/page-comments` as its folder.
- When the page needs a URL, serve it by the `serve-local` block, and give the owner the URL in chat with the round.
- For each note, change the page with the block that built it, run the page's checks again, and answer the note with the change and the check result. Then start the next round.
- The step ends when `comment_round.py check` passes on the page's files, or at the loop stop of `comment-round`.
- An approved page never allows other work to start. The `plan-review` block alone owns that gate.

## Example
Initial state: `site/index.html` passes its checks, and the owner opens round 1.
Action: the note says "the hero text is too small on the phone". Raise the size with the `build-site` block, take the 375 px shot again, and answer the note with the new size and the shot path. Round 2 returns `approved` with no notes.
Result: `comment_round.py check <run>/page-comments site/index.html` prints `round 2 approved these files`.

## Check (script)
- none: the round files live in the run folder, and the `comment-round` block's check reads them.

## Check (judge)
- For each note, does its answer name the change and the check that ran after it? Quote one note, its answer, and the check output.
