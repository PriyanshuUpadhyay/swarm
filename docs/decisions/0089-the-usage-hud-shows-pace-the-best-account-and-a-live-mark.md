---
status: accepted
date: 2026-10-15
deciders: [user]
amends: ["0087"]
informed-by:
  - 'owner, 2026-10-15: "The usage hud right now is useless... adding the claude or codex animation would be much better ... it''s at 100.. i know that it''s 100 but other have info... The older one had the information that it might run out in these many hours ... Codex is having fable table. Why the fuck does it need that?"'
  - 'owner answers 2026-10-15: closed HUD "Live mark + best account"; pace "Under each limit"'
  - 'owner, 2026-10-15: "meh it''s looks shitty.. check notchi... you can copy their setup for standby mode"'
  - "Notchi (github.com/sk-ruban/notchi, GPL-3.0): idea only"
  - "yelo apps/UsageHUD trend math, ported in 300b10bc and removed in 8238bbd8"
---

# 0089. The usage HUD shows pace, a usage ring and a live mascot

## Context and Problem Statement

The closed HUD showed the worst window, so an account the owner already knew was full read `~100%`.
The open card had no pace, and every provider shared the Session, Week and Fable columns, so Codex
rows had a Fable column that Codex has no limit for.

## Decision Outcome

- The closed HUD follows Notchi's standby idea with swarm's own art (Notchi is GPL-3.0, so none
  of its code or sprites). The left wing shows a pixel mascot of the Claude or Codex family that
  works, waits, idles, or sleeps after 5 minutes with no agent working or waiting
  (`swarm working --json`, polled every 3 s). The right wing shows a 15 pt ring with the used
  percent of the best account's fullest limit, coloured by severity, dimmed when stale and full red
  when every account is full. The wings carry no text.
- Each limit in the open card adds a pace line: `full in 2h 10m`, `lasts to reset` or `full now`.
  Session pace is the last hour's burn since the last reset; a weekly pace is the whole window's
  average, and none shows before 24 h of the window. Samples stay 7 days in
  `~/Library/Application Support/SwarmHUD/history.json`.
- The card has one group per provider, and each group shows only the limits its accounts have.
