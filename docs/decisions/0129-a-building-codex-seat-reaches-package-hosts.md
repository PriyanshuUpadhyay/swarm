---
status: accepted
date: 2026-10-27
deciders: [user]
related: ["0128"]
informed-by:
  - "A/B/C test, 2026-10-26 to 2026-10-27; ~/.claude/reports/2026-10-26-abc-test/notes.md"
  - "owner, 2026-10-27, in chat: allow network for code seats"
---

# 0129. A building Codex seat reaches the package hosts, and only those

## Context and Problem Statement

The owner's Codex configs turn on `features.network_proxy` with a short domain allowlist. A Codex
seat's `swarm-code` profile sets `network.enabled`, but the proxy still refuses every other host
with a 403. In the A/B/C test, `code.complex` seats could not fetch SwiftPM packages, the VAD model,
or pytest, so the lead fetched them by hand. `uv` also failed because its cache, `~/.cache/uv`, is
outside the seat's writable roots.

## Considered Options

- Package hosts for building seats in swarm's seat profile (chosen).
- Package hosts in the owner's global proxy allowlist: every Codex session, also review and
  council seats, would reach them.
- No change: the lead keeps fetching dependencies.

## Decision Outcome

Chosen: a seat whose role is `code.*`, `build`, or `checker` gets `network.domains` in its
`swarm-code` profile for PyPI, crates.io, npm, GitHub, and Hugging Face, and `UV_CACHE_DIR` in the
system temp folder, which the sandbox lets it write. Other seats keep the owner's allowlist only.
`codex sandbox` with this profile reached each host, refused `example.com`, and ran
`uv run --with pytest`.

### Consequences

- Good: a coding seat installs and tests on its own, so the lead does less setup.
- Bad: GitHub and Hugging Face accept uploads, so a building seat can send data out to them.
- Bad: the host list is fixed in code; a new package host needs a swarm change.
- Bad: domain rules work only while the owner's proxy is on; with it off, the network is open.
