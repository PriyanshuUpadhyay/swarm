---
status: proposed
date: 2026-10-09
deciders: [chair]
related: ["0004", "0005", "0018", "0032", "0033", "0057"]
informed-by:
  - "Owner L4 and L5 on 2026-10-06, unify-swarm/03-contracts.md:404-410"
  - "Chair decisions in swarm messages 1655 and 1659 on 2026-10-09; the owner can overturn this proposed record"
  - "Chair answer in swarm message 1644 on 2026-10-09: ADR 0004 owns most usage left; remove yelo urgency when pick moves into swarm"
  - "Chair-approved fx-accounts-2 login rule: close on success and keep failed login text visible"
  - "Local yelo source src/profile.rs:751 uses quota/time urgency, which differs from most usage left"
---

# 0063. Swarm owns the native account list, pick and login

## Context and Problem Statement

Swarm calls yelo for account list and pick. Settings has an Accounts placeholder. The owner
requires native account ownership and login through each provider's CLI. Yelo's urgency pick can
choose a different account from the most-usage-left rule retained by ADR 0018.

## Considered Options

- Move account discovery, pick, and login into Swarm, using native homes and profile environments.
- Keep yelo as the account owner and add only a Settings view.
- Move ownership but retain yelo's urgency pick.

## Decision Outcome

Proposed from the chair's reading of L4/L5: Swarm owns account list, pick, and login. Codex
accounts are Codex homes; Claude accounts are native profiles. AGY has no Swarm account source.
Auto picks the signed-in account with the most usage left, and Settings, launch and Switch Model
use that one result. Login runs the provider's own CLI in a pane; Swarm stores no secret.
Rejected: retain yelo account ownership because L5 moves it; retain urgency because chair answer
1644 confirms the most-usage-left rule. Chair 1655 sets the least-left applicable window, name-order ties, and fresh values ahead of
quota older than 5 minutes or unknown. With no fresh quota, Auto retains the current account.
Chair 1659 defines current as active CODEX_HOME or CLAUDE_CONFIG_DIR, then the native default,
without a new persistent selection. Unavailable auth proves nothing about sign-in.
Chair 1655 accepts the new login/reset/refresh commands and JSON states/units in flow
03-contracts. Register HOME/.codex-NAME or HOME/.claude/.profiles/NAME metadata before pane open.
Login opens a dedicated pane in the current workspace. A successful login closes the pane.
A failed login leaves the pane open with its text, so the owner can read the error.
The owner can overturn these chair defaults in this proposed record before build.

This changes only ADR 0005's account boundary. Its unrelated role boundary and yelo's HUD stay.
Do not supersede all of ADR 0005 or rewrite old records as part of this draft.

### Consequences

- Good: one pick and one native home flow serve Settings, launch and Switch Model.
- Bad: Swarm must maintain native discovery and login handling; pick results can change from yelo.
