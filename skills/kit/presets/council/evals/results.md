# council results

Each result compares one Round-1 council with the role cards against one without them. Seats:
council.claude opus, council.gpt gpt-6.1-sol, council.gemini gemini-3.8-flash-high.

| date | skill commit | baseline | case | grader | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-05 | 7b88feb8 | same brief, no cards | graph-store | checker codex | loss | both 4/4; the pre-mortem seat wrote a six-month failure history, crashes, and "100%" outages as events the report does not hold |
| 2026-11-05 | 7b88feb8 | same brief, no cards | full-flow-always | checker claude | win | cards 4/4, no cards 3/4: only the cards side named a short route for small changes; its pre-mortem seat again wrote a stall and killed ports as events |

Verdict 2026-11-05: fix. Fix target: the pre-mortem card wrote risks as past events in both cases.
Fix: the card now says each risk is a possible cause, never an event, and needs a pinned line.

Notes:
- All 12 seats gave NO-GO, and both sides caught each planted false claim. The cards changed the
  quality of the reasons, not the verdict.
- Positions with cards were about twice as long. The grader brief said length is not a merit.
- The grader could see "proven, partly proven, not proven" labels from the evidence-check card, so
  the blinding was partial.
