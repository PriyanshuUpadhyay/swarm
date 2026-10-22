---
status: accepted
date: 2026-10-22
deciders: [user]
amends: ["0089"]
related: ["0087"]
informed-by:
  - 'owner, 2026-10-22: the HUD must read well for 1 Claude, 10 Claude, 1 Claude + 1 Codex or Codex only; Week sat in different columns for Claude and Codex; show which models swarm runs'
  - "~/.claude/reports/2026-10-22-hud-ui-web.md (three web-search seats, Mobbin, inspora.design, evilbuttons.com, designeer.xyz)"
  - 'owner, 2026-10-22, on three mockups (A Ledger, B Tiles, C Matrix): "A. but mascot can be given a place in the expanded ui maybe top right or left"'
  - 'owner, 2026-10-22, on the notch states: "yes not states go in"'
  - "Notchmeter (notchmeter.com): the room-on-the-other-provider line, idea only"
---

# 0112. The usage HUD shows one ledger table and the running models

## Context and Problem Statement

ADR 0089 gave each provider its own group with only its own limits. With Claude and Codex both
signed in, Week was the second column for Claude and the first for Codex, so the eye could not
compare them. A card with one account or ten used the same two-line rows and a legend band, and
three expired tokens took three rows. The closed wings carried no text, so the notch could not say
which models swarm runs.

## Considered Options

- A Ledger: one table, one column per limit kind (chosen).
- B Tiles: one tile per account.
- C Matrix: accounts as columns, limit kinds as rows.

## Decision Outcome

- The open card is one table with one column per limit kind any account has, in the order
  Session · 5h, Week, Fable week. A provider without a kind shows a dim dash, with "No Week limit"
  on hover. A provider mark starts each row; Claude rows come first.
- Above 5 rows each row drops to one line, 34 pt, and a warning pace takes the reset's place. A
  Codex row's resets then move to the name's tooltip and spoken label. Past the 660 pt cap, from
  about 15 accounts, the rows scroll.
- Accounts with no reading fold into one line, such as "3 accounts need sign-in: default,
  personal, spare", with one "Renew all".
- The card is as wide as its columns need, from 820 pt to 1088 pt, so a Codex-only card has no
  800 pt bar. The legend band goes.
- The header shows the mascot top left, on the same side as the closed wing, with its credit on
  hover. Beside it: "Use next <account> · N% left" and a second line naming the other provider's
  best account ("Room on Claude lab: 66% left"), or the count of accounts read when that one is full.
- A "Running now" line lists the running agents, waiting ones first: name, then "needs you",
  model and account, up to 4 chips (2 on a narrow card), then "+N". Inside each group the order is
  session and id, not the last report, so the chips and tags do not move on each tool call.
- This amends 0089's "the wings carry no text". While agents work, the left wing adds the working
  count and the right wing up to two model tags, one per provider first. When an agent waits, the
  left wing says "needs you" in amber and the right wing names the agent. At 95% or more the ring is
  red and its number shows in place of the tags. Both wings grow by the wider side, so the shape
  stays centred on the camera.
- `swarm working --json` keeps the schema `swarm.working.v1` and adds `agents`: id, session,
  provider, model, account, state. The counts come from the same list. A HUD from before this
  change ignores the field, and this HUD skips an entry it cannot read.

### Consequences

- Good: the same kind sits in one column for every provider, and a 10-account card fits in 468 pt.
- Good: the notch answers "what runs now" without opening the card.
- Bad: a ticker of each agent's current step is not possible yet, because swarm stores no step text.
- Bad: the closed wings are wider while agents run, so they cover more of the menu bar.

## Links

- Amends [0089](0089-the-usage-hud-shows-pace-the-best-account-and-a-live-mark.md).
- Mockups: tmp/flow/2026-10-22-swarm-hud-ui-revamp/mockups/index.html.
