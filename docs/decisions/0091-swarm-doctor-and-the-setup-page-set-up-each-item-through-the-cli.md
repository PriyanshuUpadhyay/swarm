---
status: accepted
date: 2026-10-16
deciders: [user]
related: ["0077", "0088", "0090"]
informed-by:
  - 'owner, 2026-10-15: doctor/setup must set up all items or chosen ones, from a UI route, with conflicts handled'
  - 'owner, 2026-10-15: "accept all recommendations" (one apply route, preview before Apply, notices as a doctor row only)'
  - "Trial on 2026-10-15: an ad hoc signed notice helper with the time-sensitive entitlement fails to launch with POSIX 163"
  - "Review-check runs range-0ce90a4-* to range-af4664e-b4a33b4-01 (APPROVE on b4a33b44)"
---

# 0091. swarm doctor and the /setup page set up each item through the CLI

## Context and Problem Statement

The owner had no one place that showed what swarm sets up (hooks, guard, trust, herdr, skills,
plugin, HUD, transcript tool, notices, UI server) and whether each one works. A fix meant a
different command for each item, and the web UI could not run any of them.

## Considered Options

- `swarm doctor [--json]` lists each item, and the page applies a `swarm setup` plan through the
  UI server.
- The web server reads and writes the setup files itself.
- A doctor that also fixes items when it runs.

## Decision Outcome

Chosen: `swarm doctor` reads, and `/setup` sets up through the CLI, because `swarm setup` already
owns the plan, the digest, and the managed record of each change.

- `swarm doctor --json` prints schema `swarm.doctor.v1`, one row per item. A row is `ok`, `setup`
  (its `group` can fix it), or `broken` (the owner must act). It exits 0 once it can read HOME
  and the swarm home.
- `/setup` shows each row, a plan's files, conflicts and skipped paths before Apply. Apply sends
  the digest of the plan that the page showed. A group with a conflict is left out of Set up all.
- One route, `POST /api/setup/apply`, takes `only` as a comma list plus a digest, or `plugin` or
  `hud` alone. Calls run with cwd set to the swarm home, in lane `setup`, under a 180 s limit.
- Only a file that changed under the plan returns 409, so the page reloads. Any other failure
  returns 400 with the CLI's own text.
- Notices are a doctor row only. A time-sensitive notice needs a paid signing identity.

### Consequences

- Good: the owner sees and fixes every item from one page, with the same plan and undo as the
  terminal.
- Good: the server never writes a setup file itself.
- Bad: the group list lives in three files (`SETUP_GROUPS`, `PLAN_GROUPS`, the page).
- Bad: the 409 rule matches CLI error text ("changed after the", `managed::CHANGED`), so a
  reworded message becomes a plain error.
- Bad: `log show` for the notices row has no timeout of its own; it runs under the 20 s doctor limit.

## Links

- Related: 0077, 0088, 0090.
