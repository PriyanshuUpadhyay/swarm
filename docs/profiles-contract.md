# Roles, accounts and usage contract

swarm owns roles, accounts and usage (ADR 0005). It wraps `agent-routing.mjs` for roles and `yelo`
for accounts and usage, and prints the JSON below. Bloom reads only this JSON, through the Swift
types in `ui/Sources/BloomCore/Agent/SwarmProfiles.swift`. A change to a shape changes this file,
the Rust output, and the Swift types in one commit.

## Rules

- Every command prints one JSON object on stdout and exits 0. A failure prints one line on stderr
  and exits non-zero.
- Keys are `snake_case`. A value swarm cannot know is `null`, never a missing key.
- Multiple accounts are a general rule for every provider. A provider with no account source today
  (`agy`) returns `"source": null`, `"accounts": []` and `"auto": null`, and the UI launches it on
  the CLI's default home.
- swarm finds the tools through `SWARM_ROUTING_CMD` (default `$HOME/.claude/scripts/agent-routing.mjs`)
  and `SWARM_YELO_CMD` (default `yelo` on `PATH`). A missing tool is a failure with a clear stderr
  line, not an empty list.

## `swarm roles --json`

One entry per route in `roles.json`, resolved to its first runner.

```json
{
  "roles": [
    {
      "role": "code.complex",
      "runner": "codex-sol-high-agent",
      "provider": "codex",
      "model": "gpt-5.6-sol",
      "effort": "high",
      "sandbox": "workspace-write",
      "fallbacks": []
    }
  ]
}
```

`fallbacks` lists the other runner ids of the route, in order. `effort` and `sandbox` are `null`
when the runner has none.

## `swarm accounts --provider <claude|codex|agy> --json`

```json
{
  "provider": "claude",
  "source": "yelo",
  "accounts": [
    {
      "name": "sid",
      "email": "someone@example.com",
      "home": "/Users/me/.claude/.profiles/sid",
      "env": { "CLAUDE_CONFIG_DIR": "/Users/me/.claude/.profiles/sid" },
      "signed_in": true,
      "remaining_pct": 52,
      "summary": "5h 98% left · 7d 52% left"
    }
  ],
  "auto": "sid"
}
```

`env` is exactly what a launcher sets to run the CLI on that account: `CLAUDE_CONFIG_DIR` for
Claude, `CODEX_HOME` for Codex. `auto` is the account `yelo profile pick` chooses, the one with the
most usage left (ADR 0004), or `null` when no account is signed in.

## `swarm usage --json`

```json
{
  "meters": [
    {
      "provider": "claude",
      "account": "work",
      "label": "cl·work@example.com",
      "window": "7d",
      "used_pct": 10,
      "resets_in": "4d22h",
      "state": "ok",
      "as_of": 1789576942
    }
  ]
}
```

`account` is the account `name` from `swarm accounts`, matched by email, or `null` when no account
matches. `window` is yelo's window name (`5h`, `7d`, `fb`). `state` is yelo's state (`ok`, `stale`).

## `swarm spawn <agent_id> <role> [--provider <p>] [--account <auto|name>] [-- <cmd>...]`

Without `--account`, spawn is unchanged. With `--account`, swarm resolves the account for the
provider (from `--provider`, else from the route named by `<role>`), sets that account's `env` on
the child command, prints the pane id on stdout as before, and prints `account <name>` on stderr.
`auto` uses the `auto` account and fails when it is `null`. An unknown name fails before any pane
opens.
