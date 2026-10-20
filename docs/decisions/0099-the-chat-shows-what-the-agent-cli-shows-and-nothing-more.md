---
status: accepted
date: 2026-10-20
deciders: [user]
related: ["0021", "0070"]
informed-by:
  - 'owner, 2026-10-20: accepted at cli-mirror close ("accept al")'
  - 'owner, 2026-10-19: chose option 1 ("1 ofc"), the chat mirrors what the CLI shows, transformed; an embedded terminal was rejected'
  - 'owner, 2026-10-20: approved plan tmp/plan/2026-10-20-cli-mirror/plan.md (round 1, approved, no notes)'
  - 'commit c8f67c6e (2026-10-19): every log record with no row of its own became a collapsed row; no ADR'
  - 'probe 2026-10-20: a Claude AskUserQuestion with long options scrolls its question off an 80x24 pane, and the listing reports no prompt'
---

# 0099. The chat shows what the agent CLI shows, and nothing more

## Context and Problem Statement

Commit c8f67c6e made every log record visible: ignored, unknown, usage, queue, and turn records,
and hook, model, and cwd notices, each as a closed line in the chat. The CLI shows none of them, so
the chat filled with lines such as `ignored · api-request` and `usage · context`. At the same time
the chat showed less than the CLI in one place. When a Claude question was taller than the pane,
the screen lost its top, the listing found no prompt, and the composer said "Answer it first in its
pane", a pane the owner cannot see. The sidebar and tab rows also showed a raw token sum that the
CLI never shows.

## Considered Options

- The chat shows what the CLI shows, transformed: rows the CLI hides are not drawn, and a question
  the CLI shows is a card with buttons.
- Keep every record visible as a closed line, and add a toggle to hide them.
- Embed the CLI terminal in the chat area.

## Decision Outcome

Chosen: the first option, because the owner works in the chat and the CLI already decides what a
person needs to see. The server keeps marking the rows the CLI hides (`collapsed`), and the web does
not draw them or find them. A question keeps the screen as proof that a choice list is live; when
the screen lost the top of a Claude question, the open AskUserQuestion in the agent's log fills in
the text and options, a named gap under ADR 0070. Sidebar chat rows and tabs show the status
glyph, title, and unread dot by default; other fields stay available as choices.

### Consequences

- Good: the chat reads like the CLI, and a tall question gets buttons instead of a lock.
- Good: no new answer path; a click still types the option's digit into the pane.
- Bad: a record that the CLI hides cannot be read in the app any more; the log file is the place
  to read it.
- Bad: the question card for a cut Claude prompt depends on the log format of AskUserQuestion, so a
  Claude change to that format falls back to the old lock until the reader learns it.
