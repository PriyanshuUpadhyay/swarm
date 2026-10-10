---
status: accepted
date: 2026-10-07
deciders: [user]
supersedes: "0053"
related: ["0008", "0021", "0032", "0052"]
informed-by:
  - "Owner answer on 2026-10-07: tokens for all, cost if reported"
  - "Review range-dc78b5d-07d1a57-01 (refs seat, host.rs:209): Claude Code writes `cost-state` only at process exit, so the Stop hook rarely sees a current cost"
---

# 0061. The agent row keeps a running token total, and a cost only when the CLI reports one

## Context and Problem Statement

ADR 0053 made the Stop hook copy the CLI's cumulative cost to the agent row. Claude Code writes
that cost only when its process exits, so a live Claude chat had no cost and a resumed chat had
an old one. Codex reports a running token total at every turn but no cost.

## Considered Options

- A running token total for every provider, and a cost only when the CLI reports one.
- A price table in swarm that turns tokens into cost for every model.
- Keep the cost-at-exit behavior of ADR 0053.

## Decision Outcome

Chosen: a token total for every provider. `swarm launch` still writes profile, runner label, model,
effort and account to the agent row, as ADR 0053 said. At each Stop the hook reads the log from the
byte offset it reached last time and adds each new assistant message's usage once (input, cache
creation, cache read and output tokens, one count per message id) for Claude; for Codex it copies
the newest cumulative `total_token_usage.total_tokens`. A cost is stored only from a complete
`cost-state` record. A model value of `<synthetic>` is never stored. Rejected: a price table,
because swarm would have to follow every provider's price change; cost at exit, because a live chat
showed nothing.

### Consequences

- Good: every live chat shows a current token count, and no price table needs upkeep.
- Bad: a Claude row shows a cost only after its process exits, and the agent row keeps a log offset.
