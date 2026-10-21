---
status: accepted
date: 2026-10-22
deciders: [user]
amends: ["0076"]
related: ["0109"]
informed-by:
  - "~/dotfiles/docs/council/2026-10-21-skills-page.md (council verdict GO-WITH-CHANGES, unanimous, 6 required changes)"
  - "~/.claude/reports/2026-10-21-skill-graph-layout/report.md (Pele layered.ts 14 kB gzip and 0.017 ms per graph; elkjs 441 kB and 3.8 ms)"
  - 'owner, 2026-10-22: approved the plan tmp/plan/2026-10-22-skill-graph in Plannotator, then "start implementing with auto"'
---

# 0111. The web app shows the kit's skills from the kit's own script

## Context and Problem Statement

ADR 0076 took the Skills editor out of the web UI and said `swarm serve` has no skill routes. The
owner now wants to see the kit: which blocks exist, which default skill (preset) uses each one, the
steps of each preset as a graph, and a way to draft a new preset table that the kit accepts. The
kit's facts live in `skills/kit/`, and `preset.py` already parses Needs, resolves blocks, and gives
each step its stage. A second parser in TypeScript would drift from it.

## Considered Options

- A Skills dialog that reads every fact and every draft check from `preset.py`, through two
  read-only serve routes, and draws the graph with Pele's layered layout.
- The same page with the parsing in TypeScript and elkjs for the layout.
- No page; the owner reads the SKILL.md files.

## Decision Outcome

Chosen: the dialog on `preset.py`, because the script that runs the presets is then also the one
that describes them, so the page cannot disagree with a run. `preset.py --json` gives the blocks
and presets (schema `swarm.kit/1`), and `preset.py --check` reads a draft on stdin and gives the
first defect. `swarm kit --json` and `swarm kit check` run it with `python3 -I -B`, and `swarm serve`
adds `GET /api/kit` and `POST /api/kit/check` behind the same guard, time limit, and body limit as
its other CLI calls. Nothing is written: Copy puts the checked text on the clipboard, and is on
only while the latest check passed that exact text. A preset with a `Start:` line runs its own
steps, so the Composer does not start from it. The graph code is Pele 1.8.0
`src/layout/layered.ts`, copied in as is under MIT, because the npm package does not export it.
Each edge's minimum length is the stage gap, so each step sits in the row of its Python stage.

### Consequences

- Good: the page and a run agree by construction, and a vitest and an e2e test check every kit
  preset for stage rows and for edges through cards.
- Good: the chat app does not load the page; its chunk is about 17 kB gzip.
- Bad: the page needs Python 3.11 or newer on the server's PATH, and shows the script's error
  without it.
- Bad: a copied layout file does not get Pele's fixes; it changes only when a newer release is
  copied in.
- Bad: the Composer drafts text only; the owner still pastes it into a SKILL.md.

## Links

- Amends ADR-0076 (its 2026-10-13 amendment: "`swarm serve` has no skill routes").
