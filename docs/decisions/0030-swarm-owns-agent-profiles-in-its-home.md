---
status: accepted
date: 2026-09-30
deciders: [user]
related: ["0005", "0027"]
informed-by:
  - "User answer on 2026-09-30 (flow 01-frame): Swarm owns one config file with its schema in this repo; the old roles.json is imported once"
  - "User answer on 2026-09-30 (flow 04-impact): link ~/.swarm/profiles.json into dotfiles to keep sync between Macs"
  - "dotfiles ADRs 0009 and 0015, which made agent-routing roles.json the launch policy source"
---

# 0030. Swarm owns agent profiles in its home

## Context and Problem Statement

Roles lived in `~/.config/agent-routing/roles.json`, a dotfiles file whose schema was owned
outside this repo, with runner ids shared between routes. Chat picks lived in UserDefaults. The
app could edit only a shared runner's model, so one edit changed every route that used it.

## Considered Options

- Keep `roles.json` as the owner and add editing on top of it.
- One file for every build, `~/.config/swarm/profiles.json`.
- One file per swarm home, `$SWARM_HOME/.swarm/profiles.json`, with the old file imported once.

## Decision Outcome

Chosen: one file per swarm home. A profile owns its ordered runners by value, so no runner is
shared. The schema and validation live in `src/profiles.rs`. With no file, the first read imports
the old roles.json if it exists, writes `profiles.json`, and never writes the old file. With
neither file, the built-in `default-profiles.json` is used and nothing is written. A per-home file
follows ADR 0027, so a branch build cannot change the owner's real profiles.

### Consequences

- Good: the app edits the whole profile, and one schema owner checks it.
- Bad: a dotfiles edit to roles.json after the import is ignored. Swarm prints a warning when the
  old file is newer. The owner keeps sync between Macs by linking `profiles.json` into dotfiles;
  save writes through a link.
