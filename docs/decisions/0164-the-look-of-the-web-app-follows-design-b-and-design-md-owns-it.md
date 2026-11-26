---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0075", "0076"]
related: ["0060", "0152"]
informed-by:
  - 'owner, 2026-11-24: "B is sweet yeah."'
  - 'owner, 2026-11-25: "Yes, put the design files and all the decisions in the repo"'
  - 'owner, 2026-11-26: "Keep the design and related rules in a file as well, please."'
  - 'owner, 2026-11-24: "your reommendation is good" (a glyph, not a blue dot, for a running child).'
  - 'owner, 2026-11-24: "your recommendation is cool" (the glyph wave, 1.8 s) and "Keep the serif (recommended)".'
  - 'owner, 2026-11-24: "these colors look good" (darker state colours in the light theme).'
  - 'owner, 2026-11-26: "defaults are okay" (the 16 small values).'
  - 'owner, 2026-11-26: "2 3 4 5 6 recommended", the answer to the Align question 6 of the build run, which is about keeping the font choice in Settings > Appearance with the design faces first. No other owner quote names the font choice.'
---

# 0164. The look of the web app follows design B, and `DESIGN.md` owns it

## Context and Problem Statement

ADR 0075 says the look follows conductor.build, with shadcn/ui components. ADR 0076 keeps the lean
web app and says to build a short Conductor-shaped feature list "with jean's look". The colour
values of `web/src/styles-base.css` follow jean today. In a design run the owner chose direction B
(a quiet surface, one signature) on boards, and asked for the design to live in the repo.

## Decision Outcome

`DESIGN.md` at the repo root owns the look of the web app: colours, type, spacing, motion and
components. `docs/design/` holds the rules (`RULES.md`) and the final `.fig` boards. A conflict
between the code and `DESIGN.md` is a bug in one of them. This record amends only the look part
of 0075 and 0076; the web app on `swarm serve`, the feature scope and the shadcn/ui parts stay.

- Colour is kept for agent state. Yellow `#fcd452` is the primary action and the lit glyph dots.
  Focus is a 2 px ring in the text colour. Both themes use warm greys.
- Fonts are DM Sans (interface), JetBrains Mono (code, branches, chips, timers) and Instrument
  Serif (only the new-chat question). They ship as files inside the binary. Settings > Appearance
  keeps the font choice; the design faces come first and are the default.
- The seven-dot glyph is the one moving mark: a wave of 1.8 s, and a still form under
  `prefers-reduced-motion`.
- A cut always says how much is left out (`… +N lines`, "Show all").
- The jean notice in `web/NOTICE` stays while jean-derived code stays.

## Consequences

- Good, because one file owns the look, so a builder does not read boards to find a value.
- Good, because the boards stay in the repo as pictures of the rules.
- Bad, because the boards can differ from the file. `RULES.md` lists the known cases, and the
  file wins.
- Bad, because the font files add to the binary. A saved font pick is a string in the browser; an
  unknown value falls back to the default, so removal needs no data migration.
- Each later run that builds a screen states which part of `DESIGN.md` it follows. This record
  decides no screen.
