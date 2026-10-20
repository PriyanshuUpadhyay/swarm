---
status: accepted
date: 2026-10-21
deciders: [user]
amends: ["0088"]
related: ["0040"]
informed-by:
  - 'owner, 2026-10-21: "create local website", then "it would be using a local website always", and a kit block for it'
  - "council 2026-10-21 page-shelf, GO-WITH-CHANGES unanimous at round 3 (~/dotfiles/docs/council/2026-10-21-page-shelf.md)"
  - "research 2026-10-21 local-page-block: no shipped static server checked Host; python http.server, miniserve and caddy followed a symlink out of the root"
  - "hand trial 2026-10-21: self-contained pages run under `sandbox allow-scripts` in Chromium and WebKit; storage, sibling fetch and module scripts break"
---

# 0101. Agent pages go to a loopback shelf served by swarm pages

## Context and Problem Statement

Agents make HTML pages for the owner: prototypes, charts, report and plan pages. The owner wants
each one as a local URL on this Mac, not a claude.ai artifact and not a file path. Swarm's own
server keeps its bearer token in the page's local storage (ADR 0088), so an agent page on that
origin could read the token. A trial found that no common static server checks the Host header,
and three of four followed a symlink out of their folder.

## Considered Options

- A `/pages` route on the `swarm ui` server, a separate hub on its own port, or one server per page.
- Serve pages from where agents write them, or copy them into one store.
- A sandbox header on every page, none, or a per-page opt-out.
- A shipped server (python http.server, miniserve, caddy, serve-mcp) or a small hub in the swarm binary.

## Decision Outcome

Chosen: `swarm pages`, a hub in the swarm binary on its own fixed port, with a copy store and a
sandbox on every page, because it keeps agent pages off the token origin and puts each safety rule
in code.

- `swarm pages publish <folder> [--slug <slug>]` copies the regular files of a folder with a top
  `index.html` into `<root>/pages/<YYYY-MM-DD>-<slug>/`, refuses symlinks, skips dot entries, starts
  the hub when it does not answer, and prints the URL, a sha256 of the copy, the source, and the
  commit. A second publish of the same date and slug replaces the copy, so the URL stays.
- The hub binds 127.0.0.1 on port 4748 (`--port N` overrides; a busy port is an error), answers
  GET and HEAD only, refuses any Host but `127.0.0.1:<port>` and `localhost:<port>`, serves only
  canonical paths inside the store, and lists only its own index.
- Every response carries `Content-Security-Policy: sandbox allow-scripts allow-forms allow-popups
  allow-popups-to-escape-sandbox allow-downloads`, `nosniff`, `no-cache` and `no-referrer`, and no
  CORS header. Each page runs in an opaque origin, so one page cannot read another.
- `swarm pages stop` stops the recorded hub by the 0088 rule. No login item yet.
- Amends 0088: Swarm now has two fixed loopback ports, 4747 for `swarm ui` and 4748 for the shelf.
- The kit block `page-shelf` names the command. `serve-local` keeps only a project's own dev server.
  A guard rule (ADR 0040) denies the Artifact tool until the owner removes it.

### Consequences

- Good: a page link outlives its worktree and a hub restart.
- Good: a DNS-rebinding site cannot read the shelf, and an agent page cannot reach Swarm's token.
- Bad: a page that needs local storage, module scripts or a fetch of its own files breaks on the
  shelf and must use its project's dev server.
- Bad: a new day gives a new URL for the same slug, and the store grows until a clean-up exists.

## Links

- Amends: 0088. Related: 0040.
