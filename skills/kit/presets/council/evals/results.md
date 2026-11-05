# council results

Each result compares one Round-1 council with the role cards against one without them. Seats:
council.claude opus, council.gpt gpt-6.1-sol, council.gemini gemini-3.8-flash-high.

| date | skill commit | baseline | case | grader | result | grader note |
|---|---|---|---|---|---|---|
| 2026-11-05 | 7b88feb8 | same brief, no cards | graph-store | checker codex | loss | both 4/4; the pre-mortem seat wrote a six-month failure history, crashes, and "100%" outages as events the report does not hold |
| 2026-11-05 | 7b88feb8 | same brief, no cards | full-flow-always | checker claude | win | cards 4/4, no cards 3/4: only the cards side named a short route for small changes; its pre-mortem seat again wrote a stall and killed ports as events |
| 2026-11-05 | c72d2d11 | same brief, no cards (first run's positions) | graph-store | checker codex | loss | both 4/4; the pre-mortem seat called risks "likely" with no pinned line and said "all planning tasks fail immediately"; the baseline marked its cost claim as an inference |
| 2026-11-05 | c72d2d11 | same brief, no cards (first run's positions) | full-flow-always | checker claude | win | cards 4/4, no cards 3/4: the pre-mortem seat gave the short route, cited to RESULT.md:27; the baseline also had a wrong score fact |

Verdict 2026-11-05, first run: fix. Fix target: the pre-mortem card wrote risks as past events in both cases.
Fix: the card now says each risk is a possible cause, never an event, and needs a pinned line.

Verdict 2026-11-05, rerun: fix again. Risks are no longer written as events, but the Gemini pre-mortem seat
still overstates how likely and how bad they are, and that alone lost `graph-store`.

Notes:
- All 12 seats gave NO-GO, and both sides caught each planted false claim. The cards changed the
  quality of the reasons, not the verdict.
- Positions with cards were about twice as long. The grader brief said length is not a merit.
- The grader could see "proven, partly proven, not proven" labels from the evidence-check card, so
  the blinding was partial.
