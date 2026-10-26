---
status: accepted
date: 2026-10-24
deciders: [user]
related: ["0029", "0040", "0120"]
informed-by:
  - "Claude Code 2.1.310 live probes, 2026-10-24"
  - "https://code.claude.com/docs/en/hooks"
---

# 0121. Claude permission answers use a short-lived hook handoff

## Context and Problem Statement

The app answers a Claude permission through `swarm answer`. The existing command
presses terminal keys. ADR 0120 calls for Claude's `PermissionRequest` hook to carry
that answer. Questions and setup screens still need keys.

A live probe showed that Claude displays its dialog while the hook runs. A terminal
answer wins, and Claude ignores a later hook decision. Claude sends SIGTERM at the
registered timeout. A timeout or `{}` leaves the dialog usable. Hook suggestions
are not in the same order as the screen's choices.

## Considered Options

- Store pending requests in the bus database and accept an answer with a conditional update.
- Keep short-lived request files in the session's run folder and claim one answer with an exclusive file create.
- Continue to send keys for every permission answer.

## Decision Outcome

We propose request files under `runs/<session>/permission/<request-id>/`. A new
request gets a nonce, which is part of the app's opaque prompt ID. The hook keeps
its state report and waits for 50 seconds within a 60-second registration.

The hook publishes `request.json`. The answer command checks the current screen,
then creates `claim` with `create_new`, so one answer wins. It writes the full JSON
to a temporary file, then renames it to `answer.json`. The hook reads the answer
and renames it to `delivered.json`. A completion hook or deadline removes the
request folder. No bus migration is needed, so older builds can still read the bus.

An allow-once choice becomes `allow`. A persistent choice also carries the unique
matching suggestion, selected by its type and content. A deny choice becomes
`deny` with a message. A choice without a hook form closes the handoff and uses the
existing key path. The command refuses an expired or second answer. An input read
error does not send keys or grant permission.

### Consequences

- Good: supported permission answers reach Claude without keys, and a stale card cannot answer a new request with the same screen text.
- Good: pending permissions do not change the database version or the owner's Claude settings.
- Bad: the hook waits in a process until an answer, completion event, or deadline arrives.
- Bad: the screen still supplies the displayed choices and identifies the matching tool request; unsupported forms use keys.
- Bad: a killed hook can leave files until a later completion event or listing removes expired requests.

## Confirmation

CLI tests cover hook answers, suggestion mapping, key fallback, owner-only access,
expiry, repeated prompt text, completion, corrupt data, and two competing answers.
The live Claude check uses a temporary `SWARM_HOME`. AGY keeps its existing
`PreToolUse` guard; Codex's RPC answer path is a later phase.
