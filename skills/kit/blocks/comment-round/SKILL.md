---
name: comment-round
description: "Run one Plannotator round on a set of files, save the owner's decision with a hash of each file, and answer each note. Use when a plan or a page needs the owner's comments."
license: MIT
metadata:
  kind: rule
  owns: "one Plannotator round: the command, the saved result, the answers, and the loop stop"
  serves: "plan, view and talk"
  needs: "the files to review"
  gives: "<dir>/<N>.json and <dir>/<N>-answers.md per round"
  conflicts: ""
  users: "plan-review, page-comments"
  source: "plannotator 0.28.17 `annotate --help` (several files open as one review; --result-file needs --gate --json; a URL goes through Jina Reader unless --no-jina) and its --gate result {decision, feedback}, live round 2026-10-13; docs/blocks.md (annotate-loop is strict; loop stop)"
---

## Rules
- The script is `<kit root>/blocks/comment-round/scripts/comment_round.py`.
- Round N: run `python3 -I <script> run <dir> <file>...`. It opens the files as one review, waits for the owner, and writes `<dir>/<N>.json` with the decision and a hash of each file. It waits for a person, so start it where a tool time limit cannot stop it.
- Exit 2 with `unavailable plannotator`: tell the owner and stop. Never skip the review.
- Pass files only, never a URL. Plannotator sends a URL to an outside reader and shows it as Markdown.
- Read `decision` from the round file, never the message text. `dismissed` means the page closed, and it is never an approval.
- The page shows no replies. So write your answer to each note in `<dir>/<N>-answers.md`, below a quote of the note. The script refuses the next round, and the check refuses the approval, while a round with notes has no answers.
- The script refuses round 4. Then ask the owner in chat which notes are still open.
- `python3 -I <script> check <dir> <file>...` exits 0 only when the newest round approved these files and no file changed since. An edit after approval needs a new round.

## Example
Initial state: `plan.html`, drawn from `plan.md`, is ready.
Action: `comment_round.py run plan-review plan.html` prints `round 1: annotated, with notes`. The note says "skip 03-tokens". Write `1-answers.md` with the quote and "removed 03-tokens". Change the plan and run round 2, which prints `round 2: approved, no notes`.
Result: `comment_round.py check plan-review plan.html` prints `round 2 approved these files`.

## Check (script)
- `python3 -I <kit root>/blocks/comment-round/scripts/test_comment_round.py` prints `ok`: a round saves its hashes, notes need answers, an edit voids the approval, and a missing binary, a bad result, a URL and round 4 stop the script.

## Check (judge)
- Does each answers file quote each note of its round and name the change made, or say why no change was made? Quote one note and its answer.
