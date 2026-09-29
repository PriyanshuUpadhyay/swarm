---
status: accepted
date: 2026-09-29
deciders: [user]
related: ["0025"]
informed-by:
  - "User answer on 2026-09-29: inverted list + stable rows"
  - "docs/research/2026-09-29-prepend-scroll-stability.md"
  - "Probe on 2026-09-29: row moved -8 to 661 pt on a mid-scroll load with scrollPosition(id:); 0 pt upside down, idle and mid-scroll, older and newer rows"
---

# 0028. Show the transcript as an upside-down list

In the context of loading older messages while the owner scrolls up, facing that rows added above
a SwiftUI lazy stack moved the rows on screen during a scroll (the `scrollPosition(id:)` anchor
holds only while the view is still) and that rows which change height after they appear made the
view jump, we chose to flip the transcript scroll view and each row vertically and give it the rows
newest first, so older rows load at the logical end and new rows land at offset 0, and to cache the
parsed blocks of long messages, and neglected the top-row anchor, holding new rows until the scroll
stops, and offset correction, to keep the row the owner reads still with no anchor code, accepting
that scroll edges and anchors are mirrored in code and that text selection across rows, VoiceOver
order and keyboard paging need a check by hand.
