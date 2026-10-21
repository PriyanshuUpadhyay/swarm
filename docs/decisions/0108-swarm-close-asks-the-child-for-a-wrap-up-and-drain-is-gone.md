---
status: accepted
date: 2026-10-21
deciders: [user]
related: ["0041", "0007"]
informed-by:
  - 'owner, 2026-10-21: "swarm finish should own it yeah? why do we need swarm drain…. when swarm finish comes it should summarize the chat and then close the pane… it can ask the subagent itself to summarize what it did. why it did what, decisions, actionables etc, unresolved questions and so on and save it no?"'
  - 'owner, 2026-10-21: "got it… create a pr for the same implement with auto" (approval of the close-not-finish proposal)'
  - 'probe 2026-10-21 of ~/.swarm/swarm.db: 14 summarize jobs queued and none done, because nothing ran swarm drain or set SWARM_SUMMARIZER'
  - 'owner, 2026-10-21: "your recommendation is cool… will need track these as accepted limits" (review runs range-3407689-f750844-01 and range-3407689-a29df28-01)'
---

# 0108. swarm close asks the child for a wrap-up, and drain is gone

In the context of a child pane that the orchestrator closes, where `swarm close` saved nothing and
a child that died left a screen log for a `summarize` job that only `swarm drain` with
`SWARM_SUMMARIZER` ran, and nothing ever ran it, we chose to make `swarm close <id>...` send each
child a `wrap-up` message, wait up to 120 s for its `swarm finish`, save the reply to
`runs/<session>/<id>.summary.md`, and then close the pane. A reply to a wrap-up must hold the lines
`## Done`, `## Why`, `## Decisions`, `## Next steps`, and `## Open questions`, and `finish` refuses
one without them. A child with no reply in time leaves its screen in `runs/<session>/<id>.log` and
a note to the orchestrator; `swarm close --now` skips the wrap-up, and the app always uses it. A
child that dies gets the fallback note also after it answered an earlier ask, and the note names
its log when its own `swarm exited` hook saved one. We removed
`swarm drain`, the summarize job, and `SWARM_SUMMARIZER`, and kept the `job` table, because a drop
needs a migration and a change to the store's five-table identity check. We neglected closing the
pane on `finish`, because `finish` answers one ask and a child answers several rounds, and a log
summarizer, because the child knows its own reasons and a screen log does not. We accept that a
close waits up to 120 s, that a child that ignores the wrap-up leaves only its screen, and that the
app's own close buttons save no wrap-up.

## Known limits

The owner accepted these limits on 2026-10-21, from the review runs `range-3407689-f750844-01` and
`range-3407689-a29df28-01`. The screen reads of silent children share one 15 s limit, so a close
ends within about 140 s, inside the 180 s call timeout that the host contracts name.

- A `close` that stops during its wait (Ctrl-C or a call timeout) leaves the child owing a
  wrap-up, so `finish` refuses that child's plain replies until a later `close` ends.
- A plain `finish` sent at the same moment as the wrap-up can clear the debt, so the real
  wrap-up is not checked; `close` then waits 120 s and sends the no-wrap-up note, and the reply
  stays in the inbox.
- A store error, such as a lock held past the 5 s busy timeout, stops `close`, and the panes after
  it stay open.
- The adapter's `close` verb has no time limit, as before this change, so a stuck tmux or Herdr
  server holds `swarm close`.
