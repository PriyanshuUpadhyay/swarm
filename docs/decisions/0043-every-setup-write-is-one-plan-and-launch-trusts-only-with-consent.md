---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0008", "0029", "0036", "0040", "0042", "0045"]
informed-by:
  - "Owner answers on 2026-10-05 (flow 2026-10-05-setup-plan-all, 03-contracts): Q1 (a) new swarm setup --plan, hooks setup kept as --only hooks; Q2 (c) one standing consent for launch trust, bounded by trust_target; Q3 (a) no consent file means ask and the setup sheet opens once; Q4 (a) Claude hasTrustDialogAccepted false is a pending change"
  - "Owner answer I1 on 2026-10-06 (flow 2026-10-05-setup-plan-all, 03-contracts): use the recommendation, so a chair's folder pick is consent for that folder; show the trust prompt or the exact trust write, never answer silently"
  - "tmp/flow/2026-10-05-setup-plan-all/03-contracts.md (writers, collisions C1-C8, failure policy)"
---

# 0043. Every setup write is one plan, and launch trusts a folder only with consent

## Context and Problem Statement

`swarm hooks setup` shows a plan, a diff per file, and each conflict before it writes (ADR 0036).
`swarm launch` does not. It marks a folder trusted in `~/.claude.json` and each Claude profile,
in each Codex `config.toml`, and in AGY's `trustedWorkspaces`, with no diff and no consent (the
ADR 0008 note of 2026-09-30). Each of those files holds about 2000 such entries on the owner's
Mac. The Herdr toast and sound switch (ADR 0045) is a third writer outside the home. A chair that
opens seats with no owner present must not stop at a swarm dialog.

## Considered Options

- One `swarm setup --plan [--json] [--cwd] [--only]` for hooks, folder trust, and the Herdr switch,
  with one digest; `hooks setup` stays as `--only hooks`.
- Extend `hooks setup --plan` to carry trust and Herdr.
- Keep three verbs; the app calls three plans and merges them.

For launch consent: per folder root, per provider, or one standing consent for all providers.

## Decision Outcome

Chosen: one `swarm setup` plan and one standing consent for folder trust, because one digest
covers what the owner saw, `hooks setup` keeps working for brew users, and the safety check
(`trust_target`) is the same for every provider, so a per-provider or per-folder answer adds a
question and no safety. Per folder would block the first seat in every new scratch folder.

- `swarm setup status --json` gives `{"hooks","guard","trust","herdr"}` as booleans.
  `swarm setup --plan [--json] [--cwd <dir>] [--only <group>,...]` prints one diff per file and
  each conflict; `swarm setup --digest <d>` applies. Every write goes through the managed-edits
  module (ADR 0042) and is recorded.
- `~/.swarm/consent.json` holds `trust: standing | ask` for every build and every `SWARM_HOME`.
  An absent or unreadable file is `ask`. Writing it is itself a planned write in the `trust` group.
- With `standing`, launch writes the trust entries for a folder that passes `trust_target`, with
  no plan shown, and records them. With `ask`, a seat's launch writes nothing, prints
  `trust-pending <provider> <dir>`, the diff, and the approve command, and the pane shows the
  CLI's own trust prompt, which the app relays (ADR 0029).
- A chair launch (`orchestrator`) runs in the folder the owner picked in the app, so with `ask`
  that pick is consent for that one folder: launch writes and records its trust entries, because
  the app hides the chair's pane and nobody could answer a trust dialog there (owner answer I1,
  2026-10-06). A launch prints a `trusted <provider> <dir>` line for each trust write it made and
  a `trust-pending <provider> <dir>` line and the diff for each one it held back, so the app shows
  the exact write or the CLI's own trust question and never answers silently.
- A Mac that updates has no consent file, so it is `ask`, and the app's setup sheet opens once.
- A Claude `hasTrustDialogAccepted: false` is a pending change: the plan shows the flip to `true`,
  and standing consent covers it, because Claude writes `false` itself for a folder it saw before
  trust.
- Swarm names a group or key only with a `swarm` prefix and knows an unnamed handler by its exact
  command. A folder-trust key is swarm's only through the managed-edits record.
- The trust lock moves to `~/.swarm/trust.lock`, beside `consent.json`, because the trust files
  are global to the Mac and two homes lock two files today.

Collisions that can still happen, each with its fix:

| Collision | Fix |
|---|---|
| Codex guard trust is keyed by group index; a group added before swarm's moves it. | Swarm appends its group last; `setup status` recomputes the key, `guard` turns false, and the plan moves the key only when the record shows swarm wrote it. |
| Another tool passes Codex `-c` hooks in group 1. | Reported as a conflict with its fix (unchanged). |
| Codex `trust_level` already holds another value. | Left as it is; the plan lists it as skipped and the pane asks. |
| A running Claude rewrites `~/.claude.json` and drops swarm's entry. | The pane shows Claude's prompt; `setup status` shows the entry as missing. |
| Two builds lock different trust lock files. | One lock at `~/.swarm/trust.lock`. |
| The Herdr switch changes an owner value. | The plan shows old and new values; the record keeps the old value for revert. |
| A Claude child's trust key sits under `<cwd>/.herdr/workers`. | Tracked; rename only if Herdr claims `.herdr/`. |

### Consequences

- Good: no file outside the swarm home changes before the owner sees its diff once.
- Good: a chair with no owner present is never blocked by swarm; with `ask` its seat waits at the
  CLI's own prompt, as before pre-trust.
- Bad: after the update, every launch in a new folder asks until the owner approves the plan once.
- Bad: an owner's edit to a Codex `hooks.json` turns the guard off until setup runs again.
- Amends the ADR 0008 note of 2026-09-30: launch pre-trusts a folder only with consent.
