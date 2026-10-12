---
status: proposed
date: 2026-10-12
deciders: [user]
supersedes: "0064"
related: ["0005", "0032", "0061", "0071"]
informed-by:
  - 'owner, 2026-10-12: "swarm should hold everything and not yelo"'
  - "~/dotfiles/docs/council/2026-10-12-swarm-owns-accounts-isolation.md"
  - "yelo src/usage/fetch.rs, the code this read is ported from"
---

# 0072. Swarm reads Claude usage with the Keychain token

## Context and Problem Statement

ADR 0064 kept Claude quota behind `yelo usage show` and rejected a copy of yelo's Keychain read.
That call was the last place where swarm ran yelo. The owner decided that swarm owns everything
yelo owned.

## Considered Options

- Read the token and call the usage endpoint in swarm.
- Keep the yelo call.
- Use a long-lived `CLAUDE_CODE_OAUTH_TOKEN` that swarm stores.

## Decision Outcome

Chosen: swarm reads the OAuth access token of each account from the Keychain with `security`, and
calls `GET https://api.anthropic.com/api/oauth/usage` with `curl`. The headers go to `curl` on
stdin. The token stays in memory; it is in no argument list, log, error text or cache file. Swarm
never refreshes a token. The timeout comes from the caller's deadline, and there is no retry; the
cache `root_dir/claude-usage.json` is the fallback. When the Keychain entry's
`claudeAiOauth.expiresAt` (milliseconds) is in the past, swarm makes no call and gives one meter
with state `failed` and the reason "Token expired"; a missing or unreadable `expiresAt` counts as
live. HTTP 401 or 403 with a live token gives a meter with state `failed` and the reason "Sign in
again". The usage source name is `claude_api`.

These rules of ADR 0064 stay in force: Codex quota comes from the Codex app-server, each account
shows its usage source and read state, AGY has no usage source in swarm, and a cached reading is
fresh for 5 minutes.

`swarm usage --refresh --provider claude --json` joins the Codex form. The app is the one
refresher, at most once per provider per 300 seconds.

Rejected: the yelo call, because the owner removed that dependency. Rejected: a stored token,
because swarm would then hold a secret and the token cannot be renewed without a new login.

## Consequences

- Good: swarm runs with no yelo on PATH.
- Bad: swarm reads a secret that another program owns, and the endpoint is not a documented
  public API. A change there shows as `failed`, never as zero usage.
