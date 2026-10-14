---
status: accepted
date: 2026-10-14
deciders: [user]
related: ["0063", "0071", "0074"]
informed-by:
  - 'owner, 2026-10-14: "For Pi, it should use this form to pick the provider, just like it uses yelo, and then we can remove the yelo. It should work the same way for codex as well."'
  - "The pi() function in the owner's ~/.zshrc picked its Claude and Codex accounts through yelo's shell helpers"
---

# 0081. Shell wrappers pick accounts with `swarm accounts pick`

## Context and Problem Statement

ADR 0063 moved the account pick into swarm, and `swarm run <claude|codex>` picks an account and
then starts the CLI. Pi starts its own process: it reaches Claude through a bridge that spawns
`claude`, and it reads the Codex account from `PI_CODEX_HOME`. So `swarm run` cannot start Pi, and
the owner's `pi()` still called yelo to pick both accounts.

## Considered Options

- A fixed account for each provider in `pi()`.
- `pi()` copies the pick rule in zsh with `swarm accounts --json` and `jq`.
- A swarm command that prints the account `swarm run` would use.

## Decision Outcome

Chosen: `swarm accounts pick --provider <claude|codex> [--account <auto|NAME>] [--model <model>]
--json`, because the pick rule then has one owner, and a wrapper gets the same account that
`swarm run` would start.

- The JSON is `{schema: "swarm.account-pick.v1", name, home, env, clear}`. `clear` names the
  account variables to unset first. `env` holds the variables to set. `name` is null when `auto`
  finds no usable account, and the CLI then keeps its own login, the same as `swarm run`.
- An unknown or signed-out name exits non-zero with swarm's message, and prints no JSON.
- `pi()` calls it once for Claude and once for Codex. `--profile NAME` and `--codex-profile NAME`
  pass the name, and each picks `auto` when it is not given. Pi then sets `PI_CODEX_PROFILE` and
  `PI_CODEX_HOME` from the Codex pick.
- `codex()` calls it for each command that starts a session. `--profile NAME` passes the name. A
  caller-set `CODEX_HOME` is kept, and plumbing commands such as `--version` or `mcp` pick nothing.
- The account reads run at the same time, one per home, so a named pick of the last home finishes
  inside the 2 s pick limit. In a row, a Mac with four Codex homes ran out of time at the third.

## Consequences

- Good: Pi, `codex`, `swarm run` and `swarm launch` pick by the same rule.
- Bad: a valueless `--profile` no longer opens yelo's numbered menu, so the owner must type a name.
- Bad: `codex resume`, `fork`, `login` and the other commands bound to one home need `--profile`,
  because yelo found the home that holds a session and swarm does not.
- Bad: `codex profile list|create|sessions` is gone; `swarm accounts --provider codex --json` lists
  the accounts.
- Bad: Pi's Claude bridge now uses the native Claude home with the account's Keychain entry (0071),
  so chats that yelo kept in a separate profile home are not in Pi's history.
- Bad: the `claude` wrapper in yelo's `shell.sh` still picks through yelo, so yelo stays installed
  until `claude` has a swarm form too.

## Links

- Related: 0063, 0071, 0074.
