---
status: accepted
date: 2026-09-22
deciders: [user]
supersedes:
superseded-by:
related: [0014]
informed-by:
  - migrations/0001.sql
  - src/store.rs
links: []
---

# 0015. One schema file, UUID v7 sessions, seq from 0, and a home per branch

## Context and Problem Statement

Main and `ui-bloom-temp` both added a `migrations/0006.sql` with different columns, and the
migration runner applies by count, so a binary from either branch would silently misread a
database made by the other. The live database was deleted and recreated today, so there is no
history left to upgrade. Integer session ids also collide when two machines or two homes meet,
and message `seq` started at 1.

## Considered Options

- Renumber the branch's migrations after main's 0006 and keep incremental history.
- Keep incremental history and add a migration-name table with a fail-loud lineage check.
- One schema file with the final tables written directly, a fail-loud stop on any other
  `user_version`, and one `SWARM_HOME` per development branch.

## Decision Outcome

Chosen: one schema file, because the database has no reader of its history any more. A database
at `user_version` 0 is created, at 1 is opened, and at anything else stops with "database made by
another swarm build; use another SWARM_HOME or delete it". Session ids are UUID v7 strings from the
`uuid` crate, message `seq` counts from 0 per session, `session.cwd` is required, and an agent
records its provider. `SWARM_HOME` is set by hand for each process and defaults to `HOME`; nothing
sets it from the branch or build. It is the parent of the data directory, so swarm stores data in
`$SWARM_HOME/.swarm`. A development process can use `SWARM_HOME=~/.swarm-<branch>`.

### Consequences

- Good: the schema is one readable file, and two branches can never corrupt each other's data.
- Good: a session id is unique across machines and sorts by time.
- Bad: no upgrade path exists; a database from any earlier build must be deleted.
- Bad: every caller that read the session id as an integer had to change, on both the Rust and
  the Swift side.
