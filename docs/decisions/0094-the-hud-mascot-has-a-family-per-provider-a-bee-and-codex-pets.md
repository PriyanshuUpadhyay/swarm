---
status: accepted
date: 2026-10-19
deciders: [user]
amends: ["0089"]
informed-by:
  - "plan tmp/plan/2026-10-19-more-pets/plan.md, approved by the owner in round 1"
  - 'owner, 2026-10-19: "accept."'
  - "report ~/.claude/reports/2026-10-19-hud-pets.md (open-licence pets, own pet ideas, Codex resets)"
  - "yelo commits 60cb2a5 and d5f2103: the resets line that the 300b10bc port never carried"
  - "codex-rs app-server-protocol v2 account.rs: rateLimitResetCredits; tui/src/pets/model.rs: pet.json"
---

# 0094. The HUD mascot has a family per provider, a bee and Codex pets

## Context and Problem Statement

ADR 0089 gave the mascot only a Claude and a Codex family, so an AGY or pi agent showed the Claude
pet. The Codex reader already received the account's usage limit resets but dropped them, so the
HUD lost the resets line that yelo showed. The owner asked for more pets and for that line back.

## Decision Outcome

- AGY (a twin star) and pi (a π that walks) get their own 12x10 art, and `busiest` counts them.
  Turn 1 of the cast is swarm's bee, which works in a swarm of one bee per working agent, up to 3.
- A strained best account (its ring is not calm) makes the pet sweat.
- `swarm usage --json` adds an optional `reset_credits` object, `{available, expiry_time_seconds}`,
  on each Codex meter, in schema `swarm.usage.v1`. Only `available` credits count, and a credit
  that expired since the read is dropped. A Codex row shows `3 resets · next Oct 23` under its
  name; the closed ring gets a dot when the best account is a strained Codex account with a reset.
  The HUD never spends a reset, because `account/rateLimitResetCredit/consume` cannot be undone.
- Guests come only from art with a licence we read: five more Pixel Agents workers (MIT, art CC0)
  and the Ninja Adventure frog and raccoon (CC0). Pets the owner installed for Codex,
  `$CODEX_HOME/pets/<id>/pet.json`, join the cast after the guests, credited "Your Codex pet",
  scaled smoothly; a manifest path that leaves its folder is refused.

## Rejected

- vscode-pets raccoon: its author copied art from an issue by hand. vscode-pets Akita: its folder's
  `license.txt` is CC BY-ND 4.0. vscode-pets monkey: traced from a photo of a known zoo monkey with
  a branded plush, and 246 KiB.
- A "use reset" button: one click would spend a credit for good.
- Exporting swarm's pets as Codex packages, and the resets on the web usage page: later work.
