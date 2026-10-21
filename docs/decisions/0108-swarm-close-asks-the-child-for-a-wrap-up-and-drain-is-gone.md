---
status: accepted
date: 2026-10-21
deciders: [user]
related: ["0041", "0007"]
informed-by:
  - 'owner, 2026-10-21: "swarm finish should own it yeah? why do we need swarm drain…. when swarm finish comes it should summarize the chat and then close the pane… it can ask the subagent itself to summarize what it did. why it did what, decisions, actionables etc, unresolved questions and so on and save it no?"'
  - 'owner, 2026-10-21: "got it… create a pr for the same implement with auto" (approval of the close-not-finish proposal)'
  - 'probe 2026-10-21 of ~/.swarm/swarm.db: 14 summarize jobs queued and none done, because nothing ran swarm drain or set SWARM_SUMMARIZER'
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
child that dies gets the note with its log path, also after it answered an earlier ask. We removed
`swarm drain`, the summarize job, and `SWARM_SUMMARIZER`, and kept the `job` table, because a drop
needs a migration and a change to the store's five-table identity check. We neglected closing the
pane on `finish`, because `finish` answers one ask and a child answers several rounds, and a log
summarizer, because the child knows its own reasons and a screen log does not. We accept that a
close waits up to 120 s, that a child that ignores the wrap-up leaves only its screen, and that the
app's own close buttons save no wrap-up.
