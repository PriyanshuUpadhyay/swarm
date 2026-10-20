---
name: page-shelf
description: "Give the owner a built page as a local URL from the swarm page shelf, never a hosted artifact or a file path. Use when a step makes an HTML page for the owner to see."
license: MIT
metadata:
  kind: step
  owns: "how a built page reaches the owner"
  serves: "view and talk, close"
  needs: "a built page folder with a top index.html"
  gives: "the page URL, its source path, its content hash, and the commit"
  conflicts: ""
  users: "page-comments, planner"
  source: "docs/decisions/0101-agent-pages-go-to-a-loopback-shelf-served-by-swarm-pages.md; ~/dotfiles/docs/council/2026-10-21-page-shelf.md (required changes 1-8)"
---

## Rules
- Build the page as one folder with a top `index.html`, self-contained where possible. The shelf sends each page in a sandbox, so a page that needs local storage, module scripts, or a fetch of a sibling file goes to the `serve-local` block.
- Run `swarm pages publish <folder>`. The command copies the page into the shelf, starts the shelf when it does not run, and prints the URL with the page's hash and source. Its error text says what to fix.
- Give the owner the URL line in the reply, with the hash line as the check. Publishing the same folder again on the same day keeps the URL.

## Example
Initial state: the step made `tmp/skill-blocks/index.html`, one self-contained page.
Action: run `swarm pages publish tmp/skill-blocks`. It prints `http://127.0.0.1:4748/2026-10-21-skill-blocks/`, then `hash sha256:…`, `source …/tmp/skill-blocks`, and `commit eb10708c`.
Result: the reply gives that URL and the hash. The page opens after the worktree is removed, because the shelf keeps its own copy.

## Check (script)
- `curl -s <url> | shasum -a 256` equals the printed hash, and `curl -sI <url>` shows `Content-Security-Policy: sandbox allow-scripts`.

## Check (judge)
- Does the reply give the page as the URL that `swarm pages publish` printed, and not as a file path or a hosted artifact link? Quote the URL line.
