---
status: accepted
date: 2026-11-11
deciders: [user]
amends: "0099"
related: ["0099", "0120"]
informed-by:
  - 'owner, 2026-11-11: approved plan tmp/flow/2026-11-11-ui-unmapped-items/plan.md on its Plannotator page with no notes, so every recommended default stands'
  - 'research 2026-11-11: T3 Code drops unknown item types on decode, and Swarm hid them too'
---

# 0139. The chat counts records no parser maps

ADR 0099 says the chat draws what the CLI shows. A new CLI version can add a record that no parser
maps, and 0099 hid it like any other bookkeeping, so an update went unnoticed. ADR 0120 promised
`_swarm/unmapped` for these records, but no code read it.

## Decision

- A record no parser maps (event `unknown`, or a contract event the view does not know) becomes a
  hidden row with `system_kind` `unmapped`. An `ignored` event stays `record`, because ignoring is
  a decision, not a loss.
- The web shows one quiet line at the end of the chat, "N items this app does not show yet". It opens
  each record's raw JSON. With none, there is no line.
- The count covers the rows the chat has loaded; an older page adds its records when it loads.
- This line keeps 0120's promise with a web line, not a `_swarm/unmapped` event.
- A CLI item that the CLI draws on its own screen gets a row, not a count. Pi bookkeeping entries that
  its session-format doc keeps out of model context are ignored on purpose, so they do not count.

### Consequences

- Good: a CLI update that adds a record type shows up in every chat that has one.
- Bad: the line is the one thing the chat draws that the CLI never shows, a named exception to 0099.
- Bad: a count of loaded rows can grow when the reader scrolls up.
- Bad: each new bookkeeping type must go on a parser's ignore list, or every chat shows the line.
