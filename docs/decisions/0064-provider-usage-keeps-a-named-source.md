---
status: proposed
date: 2026-10-09
deciders: [chair]
related: ["0005", "0006", "0057", "0061", "0063"]
informed-by:
  - "Owner L5 on 2026-10-06: yelo keeps its usage HUD until research shows its reads can move"
  - "Chair decisions in swarm messages 1655 and 1659 on 2026-10-09; the owner can overturn this proposed record"
  - "Chair answer in swarm message 1644 on 2026-10-09: native Codex app-server read, Claude through yelo, per-account source, AGY no source"
  - "Official Codex app-server reference: https://learn.chatgpt.com/docs/app-server"
  - "Official Claude CLI reference and commands: https://code.claude.com/docs/en/cli-reference and https://code.claude.com/docs/en/commands"
  - "Official AGY usage and headless docs: https://antigravity.google/docs/cli/commands/usage and https://antigravity.google/docs/cli/headless"
---

# 0064. Provider usage keeps a named source

## Context and Problem Statement

The unused Swarm usage reader delegates every provider to yelo. Research found an official
Codex account/rateLimits/read surface. Claude documents interactive /usage, while the inspected
CLI reference gives no matching headless quota JSON endpoint. AGY has quota display and a
headless text report, but no account adapter is established for Swarm.

## Considered Options

- Read Codex quota through its official app-server and retain Claude usage through yelo.
- Retain every usage read through yelo.
- Copy yelo's OAuth and keychain secret-reading code into Swarm.

## Decision Outcome

Proposed from chair answer 1644: Codex quota moves to Swarm through the official app-server.
Claude quota remains through yelo for now. Each account shows its usage source and read state.
AGY shows no usage source in Swarm; this does not claim AGY itself lacks quota display. Swarm
stores no secret. Rejected: keep all reads through yelo because research supports the Codex
move; copy private OAuth logic because that would move secrets and an unsupported API boundary.
Chair 1655 sets cached yelo show plus HUD refresh for Claude, and explicit native refresh
for Codex. Defaults are a 5-minute cache age and the existing SwarmCLIBus call deadline
(20 seconds at ui/Sources/SwarmCore/Agent/SwarmCLIBus.swift:262). Scheduling keeps its separate
2-second total read deadline from ADR 0032. The owner can overturn these defaults here.

The HUD remains yelo's. Settings fills G-8/G-9; ADR 0006's menu-bar requirement remains separate.
ADR 0061 token totals and CLI-reported costs do not supply plan-quota percentages.

### Consequences

- Good: a supported Codex read removes one wrapper dependency and names missing or failed data.
- Bad: sources differ by provider, and Claude quota still needs yelo; caches need an age policy.
