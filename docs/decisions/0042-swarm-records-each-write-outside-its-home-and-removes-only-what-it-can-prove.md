---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0027", "0029", "0034", "0036", "0040"]
informed-by:
  - "Owner answer on 2026-10-05 (flow 03-contracts, Q1-Q5): accepted all recommendations"
  - "Flow folder tmp/flow/2026-10-05-managed-edits (01-frame, 02-design, 03-contracts)"
---

# 0042. Swarm records each write outside its home and removes only what it can prove

## Context and Problem Statement

`swarm hooks setup` plans, diffs, and refuses conflicts (ADR 0036), but keeps no record and has no
undo. `swarm launch` writes three folder-trust entries (Codex `trust_level`, Claude
`hasTrustDialogAccepted`, AGY `trustedWorkspaces`) with no plan or record. ADR 0040 adds three
guard writers and leaves their removal to the owner, by hand. A Codex trust key is keyed by place,
not by owner, so the key alone does not show who wrote it.

## Considered Options

- A record table `managed_edit` in `swarm.db`, with exact-match revert.
- A JSON record file per swarm home, `$SWARM_HOME/.swarm/managed.json`.
- One record file for every build, `~/.swarm/managed.json`.
- No record; scan for swarm's text only.

## Decision Outcome

Chosen: a table `managed_edit` in `swarm.db` (migration 0006), because one transaction covers each
file's rows and its write, the store already proves the home (ADR 0036), and launch opens the db
already. A per-home JSON file needs its own locked read-modify-write and schema version. One file
for every build makes a branch build write into the main home, which ADR 0027 keeps apart. A scan
alone cannot find trust entries, because the CLIs write the same values themselves.

- One module, `src/managed.rs`, applies every write outside the swarm home and records each added
  item: file, kind (TOML key, JSON key, JSON array item), path, value written, value before, writer,
  time.
- Swarm never writes what it cannot record. When the store does not open, `hooks setup`, `managed`,
  and later the launch-trust writes fail before any write.
- Revert removes an item only while it equals the value swarm wrote, and restores the value before
  when there was one. Any other value is a conflict with kind (taken, changed, order, or
  unreadable), file, entry, found, wanted, and fix, and no file is written.
- An item with no record counts as swarm's only when it equals swarm's current build-neutral hook
  text (ADR 0034); `list` shows it as found, not recorded, and revert may remove it. Trust entries
  are never claimed without a record.
- For a recorded Codex folder trust, exact match is the proof: revert removes `trust_level` only
  while it still equals `"trusted"`, after the diff.
- `swarm managed list [--json]` shows each item's live state; `swarm managed revert` takes ids or
  `--all`, with `--plan` and `--digest` as `hooks setup` has them. In the app the row switch is the
  undo: Off reverts through the hooks setup sheet, and On runs the owning writer's plan again.
  Nothing is sticky, so a writer may add an item again with consent.

### Consequences

- Good: each swarm change outside its home can be seen and removed with a diff first.
- Good: ADR 0040's registrations no longer need removal by hand.
- Bad: an older build refuses the migrated db, and `hooks setup` now needs the store.
- Bad: a revert of a Codex folder trust can remove a trust the owner also gave in Codex's dialog;
  the cost is one trust prompt.
- Bad: a write made from another build's home shows only when its text proves it; a trust write of
  a branch build is not listed in the main build.
