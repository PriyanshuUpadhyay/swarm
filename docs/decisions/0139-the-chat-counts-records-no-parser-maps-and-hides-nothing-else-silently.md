---
status: accepted
date: 2026-11-11
deciders: [user]
amends: "0099"
related: ["0099", "0120"]
informed-by:
  - 'owner, 2026-11-11: approved plan tmp/flow/2026-11-11-ui-unmapped-items/plan.md on its Plannotator page with no notes, so every recommended default stands'
  - 'research 2026-11-11: T3 Code drops unknown item types on decode, and Swarm hid them too'
  - 'owner, 2026-11-11: approved the record-map audit build; every record must have an explicit end state'
  - 'owner, 2026-11-12: approved rows for Codex Extension Sleep, WebSearch, and ImageGeneration'
---

# 0139. The chat counts records no parser maps

ADR 0099 says the chat draws what the CLI shows. A new CLI version can add a record that no parser
maps, and 0099 hid it like any other bookkeeping, so an update went unnoticed. ADR 0120 promised
`_swarm/unmapped` for these records, but no code read it.

## Decision

- Every record is a row, part of a named row, hidden on purpose, or counted as unmapped. A record
  that gives no events is unmapped. Copies of an item join its row; they do not add a second row.
- `tests/record_map/mapping.rs` owns `RECORD_MAP`, the table of CLI kinds and their end states.
  Its guard in `tests/record_map.rs`
  reads every provider file in `src/view/transcript/normalize/`, rejects a CLI and full group/kind
  path with no table entry, and
  replays every row or part entry through the public `SnapshotReader`. Run `cargo test --test record_map`.
  `NotKnown` keeps a hidden Claude kind hidden when its CLI display is not confirmed. The four
  existing rows `away_summary`, `scheduled_task_fire`, `stop_hook_summary`, and
  `hook_permission_decision` keep their rows until the owner checks the CLI.
- Codex `Extension` items map by their `kind`. Sleep (`clock.sleep`) shows the wait duration.
  WebSearch (`web.search`) shows a `web_search` tool row with its query and results.
  ImageGeneration (`image_gen.generation`) joins the image row from its paired records.
  Other extension kinds stay unmapped.
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
