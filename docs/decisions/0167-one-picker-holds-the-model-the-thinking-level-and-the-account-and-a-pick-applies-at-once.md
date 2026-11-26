---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0112"]
related: ["0110", "0122", "0168"]
informed-by:
  - 'owner, 2026-11-25: "model picker as well….. t3 codes has a sweet as model picker we can revamp that as well"'
  - 'owner, 2026-11-26: "We can have the pending state not be an issue."'
  - 'owner, 2026-11-26: "Yes, defaults okay." (a pick applies at once; one popover for the model, the thinking level and the account; the key ⇧⌘M; the key ⌃⌘M; the CLI name when there is one account).'
  - 'owner, 2026-11-26: "Yeah, this looks good. Approved." (the picker board of round 20).'
---

# 0167. One picker holds the model, the thinking level and the account, and a pick applies at once

## Context and Problem Statement

ADR 0112 gives the run chip a popover that shows notes, limits and cost. It blocks a switch while
the agent works: `applyBlock` says "Wait for this turn to end, or stop it." It also has an Apply
button with labels such as "Restart and apply" and "Start new chat". The owner finds the
switching heavy and wants a picker like the one in t3 code.

## Decision Outcome

- One picker opens from the run chip and with ⇧⌘M. It has a rail of CLIs ("Favorites" and one
  item per CLI), a search field, a "Recent" group and a list. It is 560 px wide.
- A pick applies at once. There is no "Apply" or "Restart and apply". One line at the foot says
  what the pick does. A pick is for this chat; "Use for new chats" sets the new-chat model.
- While the agent works, a pick does not apply and does not wait. The picker says so and offers
  "Stop". When the turn ends, a banner offers "Switch now" and "Dismiss". The pending pick lives
  in the browser, and a reload drops it.
- ⌃⌘M goes to the next favourite of this CLI. The app never changes the model by itself. A model
  that the CLI no longer lists stays selected with the tag "unavailable".
- The run chip reads `account · model · thinking`. It shows the CLI name when there is one account.
- Favourites and recent models are new stored owner choices (`prefs.favoriteModels`).

## Consequences

- Good, because one place changes the model, the thinking level and the account.
- Good, because a pick has no second step.
- Bad, because a pick made while the agent works is lost on reload. This is on purpose: no switch
  state lives on the server (the aim of 0112).
- Bad, because favourites and recent models are new stored data.
- The picker is built in run 4. A switch to another CLI is in ADR 0168. The marks of the CLIs are
  in ADR 0171.
