---
status: accepted
date: 2026-10-24
deciders: [user]
related: ["0042", "0117"]
informed-by:
  - "Agent contract matrix, 4 CLIs on a raw PTY (report draft 3, 2026-10-24)"
  - "Owner chose the AGY mode always-proceed plus a PreToolUse hook on 2026-10-24"
---

# 0120. Swarm drives agent CLIs through one ACP-shaped contract over a raw terminal

## Context and Problem Statement

Swarm runs Claude Code, Codex, pi, and AGY in a raw terminal (PTY) that swarm owns.
Each CLI reports its state through different channels: hooks, transcripts, OSC 7501,
an app-server, or an extension API. A probe matrix tested 8 item groups per CLI
(session, turn, output, tools, questions, sub-agents, settings, setup screens).
Every item has a structured channel, except an AGY mode change made inside the session.

## Considered Options

- One contract on ACP v1, fed by each CLI's structured channels.
- Read the screen as the main channel for every CLI.
- One adapter per CLI with its own event set and no shared contract.

## Decision Outcome

We chose ACP v1 (Agent Client Protocol) as the contract, because each CLI's structured
channels cover it and a client then needs one event set.

- Extensions use `_swarm/*` methods and `_meta.swarm` capability flags in `initialize`.
- Each adapter reads structured channels first. Claude uses hooks, its transcript, and
  OSC 7501. Codex uses a relay to its app-server. pi uses its extension API. AGY uses
  workspace hooks in `.agents/hooks.json`, its transcript, and OSC 7501.
- The host answers permissions through a hook or RPC, never a key press. Claude uses its
  `PermissionRequest` hook, AGY its `PreToolUse` hook, and Codex its approval requests.
- AGY keeps the user's `toolPermission`. Its `PreToolUse` hook gates every tool, also
  under `always-proceed`.
- The host owns the prompt queue and sends each queued prompt at turn end. Steer goes
  through Codex `turn/steer`, pi extension input, and AGY `PreInvocation` `injectSteps`.
- Every raw element maps to a contract event, maps to `[]` on purpose, or goes out as
  `_swarm/unmapped`.
- The host reads the screen only for named gaps: setup screens (`_swarm/setup_screen`)
  and an AGY mode change made inside the session.
- The host never answers a sign-in, login, or token refresh. It reports them as
  `setup.login`.

Rejected options and why:
- Screen reading as the main channel breaks on each CLI update and gives no option ids.
- AGY `toolPermission=request-review` per session: no flag sets it for one session; AGY
  reads it only from the shared `settings.json`.
- `agy remote-control`: it registers a daemon with the account.
- Gemini CLI and OpenCode: out of scope (owner).

### Consequences

- A new CLI needs one adapter and one capability set, not a client change.
- A new CLI version can add record kinds. `_swarm/unmapped` shows them at runtime.
- An adapter edits single keys in a shared config file and never restores the whole file,
  because AGY rewrites `settings.json` at every start and other sessions write the same files.
- An AGY mode change made inside the session stays a screen read until AGY reports it.
