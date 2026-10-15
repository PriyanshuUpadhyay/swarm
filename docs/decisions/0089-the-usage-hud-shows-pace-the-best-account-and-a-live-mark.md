---
status: accepted
date: 2026-10-15
deciders: [user]
amends: ["0087"]
informed-by:
  - 'owner, 2026-10-15: "The usage hud right now is useless... adding the claude or codex animation would be much better ... it''s at 100.. i know that it''s 100 but other have info... The older one had the information that it might run out in these many hours ... Codex is having fable table. Why the fuck does it need that?"'
  - 'owner answers 2026-10-15: closed HUD "Live mark + best account"; pace "Under each limit"'
  - "yelo apps/UsageHUD trend math, ported in 300b10bc and removed in 8238bbd8"
---

# 0089. The usage HUD shows pace, the best account and a live mark

## Context and Problem Statement

The closed HUD showed the worst window, so an account the owner already knew was full read `~100%`.
The open card had no pace, and every provider shared the Session, Week and Fable columns, so Codex
rows had a Fable column that Codex has no limit for.

## Decision Outcome

- The left wing shows the Claude or Codex mark. It turns while an agent of that provider reports
  `working` (`swarm working --json`, polled every 3 s), and pulses instead with Reduce Motion on.
- The right wing shows the account with the most room and its pace, with the used percent of its fullest
  limit, such as `pro2 · 2%`, and a time only when a limit fills before its reset, such as
  `pro2 · 82% · 3h left`. When every account has a full window, it shows the earliest time one is usable
  again, such as `resets 1h 20m`. Only the right wing grows, so the left side of the menu bar keeps
  the space it had.
- Each limit in the open card adds a pace line: `full in 2h 10m`, `lasts to reset` or `full now`.
  Session pace is the last hour's burn since the last reset; a weekly pace is the whole window's
  average, and none shows before 24 h of the window. Samples stay 7 days in
  `~/Library/Application Support/SwarmHUD/history.json`.
- The card has one group per provider, and each group shows only the limits its accounts have.
