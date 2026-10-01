---
status: accepted
date: 2026-10-01
deciders: [user]
related: ["0027", "0029", "0031", "0034"]
informed-by:
  - "Council 2026-10-01, docs/council/2026-10-01-install-conflicts.md (GO-WITH-CHANGES)"
  - "User answer on 2026-10-01 (flow 03-contracts): marker file; an old hook entry is a conflict with a fix"
---

# 0035. Swarm writes only where it proves ownership

## Context and Problem Statement

`swarm init` makes `runs/` and `adapters/` in `~/.swarm` before any check, so a folder that
another tool made gets swarm's files. The store only checks `user_version`, and it builds swarm's
tables in a foreign `swarm.db` at version 0. `swarm hooks setup` replaces any Codex
`trusted_hash` at swarm's key and any AGY group named `swarm`, and shows no diff first. Codex keys
trust by place, not by owner, so the key alone does not show who wrote an entry.

## Considered Options

- Move the home to `~/Library/Application Support/<bundle id>/`.
- Use an app sandbox container.
- Keep `$SWARM_HOME/.swarm` and prove ownership before each write, with a marker file.
- Keep the home and prove ownership with SQLite `PRAGMA application_id` in `swarm.db`.

## Decision Outcome

Chosen: keep the home and prove ownership with a marker file, `.swarm/swarm-home`, because
Application Support is a naming convention that another tool can also use, a brew CLI has no
bundle, branch builds share the app's bundle id, and a container needs a sandbox. A move would
also change the ADR 0034 hook text and break the linked `profiles.json` of ADR 0031.

- An empty or missing home is claimed with the marker (`create_new`) before any other write.
- A home with no marker is adopted only when a read-only open of `swarm.db` shows `user_version`
  1..=4 and the tables of migration 0001. Any other folder is refused with no write, and the
  message names the folder and `SWARM_HOME`.
- A linked home or file is judged by its target.
- A hook entry is unchanged only when it equals swarm's current text exactly. A missing entry is
  an add. Any other entry at swarm's key or group is a conflict. The list of old swarm forms that
  count as an update starts empty, because each form before ADR 0034 held a build path.
- With any conflict, setup writes no file and exits non-zero. Each conflict names the file, the
  key or group, what was found, what swarm wants, and the one-line fix.
- `swarm hooks setup --plan [--json]` writes nothing and prints a unified diff per file and the
  conflicts. The app sheet shows that plan, disables approval on a conflict, and apply refuses a
  file whose bytes changed after consent.

### Consequences

- Good: a user's other tool and own hooks are never changed without a visible diff and consent.
- Good: every build and every link reads the same proof from the thing it guards.
- Bad: a Mac that set up hooks with swarm 0.4.0 sees one conflict for each of the six Codex hook
  keys in each Codex home and one for the AGY group, and must delete the old entries by hand.
- Bad: a false refusal is possible, for example a half-made home from an older crashed `init`;
  the message must give the fix.
- Rejected `application_id` because `init` makes folders before the db, so a crash leaves a home
  with no proof, and each check would open SQLite.
