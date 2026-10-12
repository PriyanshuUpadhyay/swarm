---
status: proposed
date: 2026-10-12
deciders: [user]
related: ["0005", "0064", "0072", "0073"]
informed-by:
  - 'owner, 2026-10-12: "We should be using a standard for this so that it''s easier to work with ... something we can create or reuse"'
  - 'owner, 2026-10-12: "B", to the choice between a swarm spec, a swarm spec plus an OpenUsage export, and a switch to the OpenUsage shape'
  - "~/.claude/reports/2026-10-12-usage-account-data-standard.md"
  - "OpenUsage v0.7.14, docs/local-http-api.md; RFC 9457 section 3.1.4; claude-swap json_output.py"
---

# 0074. Usage and account JSON has a versioned format with reason codes

## Context and Problem Statement

`swarm usage --json` and `swarm accounts --json` had no version, and no written contract. The app
decided what to show from the English text of a meter's `reason`, such as "Sign in again", so a
reworded text broke the app with no error. The owner asked for a standard that swarm reuses or
writes. No existing standard covers quota meters for agent subscriptions together with the sign-in
state of an account. OpenUsage has a versioned format for meters, with one producer, no account
key, and failures as free text.

## Considered Options

- A swarm spec that reuses the public parts.
- That spec, and also an export in the OpenUsage shape.
- A switch of the wire format to the OpenUsage shape.

## Decision Outcome

Chosen: the swarm spec plus the export. `docs/usage-format.md` is the contract.

- The printed usage object has `"schema": "swarm.usage.v1"`, and the account list has
  `"schema": "swarm.accounts.v1"`. A change in `v1` only adds fields or codes. A reader ignores a
  field or a code that it does not know.
- Each meter has a `reason_code` exactly when it has a `reason`. One Rust type owns each code with
  its text, and a test ties the list to the spec. A reader matches the code and never the text, as
  RFC 9457 says for `detail`. The words `token_expired`, `relogin_required` and `no_credentials`
  come from claude-swap. Swarm adds `refresh_failed`, `read_failed`, `no_cached_usage`,
  `no_quota_windows` and `no_source`.
- The field names, the units and the state names do not change. Percent stays 0 to 100 and times
  stay seconds since 1970, as the providers give them.
- `swarm usage --format openusage` prints the cached meters as an `openusage.limits.v1` object,
  with no provider call. The provider key is `<provider>` for the account `default`, else
  `<provider>@<account>`.

Rejected: a switch to the OpenUsage shape, because it breaks the app and every script that reads
swarm, and swarm must still add the account, the read state and the codes. Rejected: OSC 7501 for
this data, because it reports the state of a live program and not a stored reading.

## Consequences

- Good: a reworded reason text cannot change what the app does, and a tool that reads OpenUsage
  can read swarm.
- Bad: swarm owns a code list that no other tool shares, and the export follows a format that has
  one producer and can change without notice.
- An older swarm build that reads a cache file with a code it does not know treats that cache as
  unreadable until the next refresh.
