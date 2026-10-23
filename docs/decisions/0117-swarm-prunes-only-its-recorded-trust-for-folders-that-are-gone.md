---
status: accepted
date: 2026-10-23
deciders: [user]
related: ["0042", "0043", "0088"]
informed-by:
  - "Owner chose option A in the trust-prune task brief on 2026-10-23"
---

# 0117. Swarm prunes only its recorded trust for folders that are gone

## Context and Problem Statement

Swarm records the folder trust it writes for Claude, Codex, and AGY under ADR 0042.
Deleted worktrees and scratch folders leave these entries in the CLI files.
An equal value alone cannot prove that Swarm wrote it.

## Considered Options

- Remove only active trust entries with a Swarm managed-edit record.
- Also scan and remove trust entries that the CLIs wrote themselves.
- Leave stale trust for manual removal.

## Decision Outcome

We chose the recorded entries only, because the owner chose option A and trust written
by a CLI must stay under that CLI's control.

Swarm starts cleanup in a separate process when its UI stops or its server exits,
and after the install and release scripts install a different package version.
The first UI server start also checks the version, so plain Homebrew upgrades are covered.
Ordinary CLI commands never start a cleanup writer alongside their own work.
Only an already claimed Swarm home with a database can schedule cleanup.
The cleanup reads only that home's active `launch.trust` records.
It uses the existing managed removal and revert commit path, with the shared trust lock.
It does not wait for a busy trust lock.

A folder is gone only when `symlink_metadata` returns `NotFound`.
A dangling link, a present path, or any other error keeps the entry.
The current trust value must still equal the recorded value.
Revert restores a previous value when one exists and marks the record off.
Changed, unrecorded, and already reverted entries stay as they are.

This automatic revert is a narrow addition to the manual plan and consent flow in
ADRs 0042 and 0043. The caller does not wait for cleanup.
Cleanup errors go to `trust-prune.log` and do not fail the caller.
A successful pass records its package version inside the Swarm home.

### Consequences

- Swarm removes stale trust it can prove it wrote, without a UI or launch waiting for the scan.
- CLI-owned trust stays in place, including stale entries without a Swarm record.
- A changed or unreadable entry stays until a later pass or manual repair.
- An equal trust value has no separate owner marker, so a later matching approval by the CLI
  cannot be distinguished from Swarm's original write, as ADR 0042 already accepts.
