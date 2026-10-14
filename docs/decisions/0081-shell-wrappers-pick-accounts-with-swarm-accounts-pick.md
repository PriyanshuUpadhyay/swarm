---
status: accepted
date: 2026-10-14
deciders: [user]
related: ["0063", "0071", "0074"]
informed-by:
  - 'owner, 2026-10-14: "claude should use swarm"; on --profile for Codex session commands: "need to fix this"; on auto pick: "auto pick can refresh stale data if it''s older than 15 mins or 5 mins or some threashold"'
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

Chosen: `swarm accounts pick --provider <claude|codex> [--account <auto|NAME> | --session <id> |
--last [--all]] [--model <model>] --json`, because the pick rule then has one owner, and a wrapper gets the same account that
`swarm run` would start.

- The JSON is `{schema: "swarm.account-pick.v1", name, home, env, clear}`. `clear` names the
  account variables to unset first. `env` holds the variables to set. `name` is null when `auto`
  finds no usable account, and the CLI then keeps its own login, the same as `swarm run`.
- An unknown or signed-out name exits non-zero with swarm's message, and prints no JSON.
- `pi()` calls it once for Claude and once for Codex. `--profile NAME` and `--codex-profile NAME`
  pass the name, and each picks `auto` when it is not given. Pi then sets `PI_CODEX_PROFILE` and
  `PI_CODEX_HOME` from the Codex pick.
- `codex()` and `claude()` call it for each command that starts a session. `--profile NAME`
  passes the name. A caller-set `CODEX_HOME`, `CLAUDE_SECURESTORAGE_CONFIG_DIR` or
  `CLAUDE_CONFIG_DIR` is kept, and plumbing commands such as `--version` or `mcp` pick nothing.
  `codex login|logout` and `claude auth` need `--profile`, because they write one account's login.
- Codex keeps each session inside one home. `--session <id>` picks the account whose home holds
  that rollout, archived ones too, and `--last [--all]` picks the account with the newest session
  in the current folder (or anywhere). `codex resume|fork|archive|unarchive|delete` pass the id or
  `--last`; a command with neither, such as the resume picker, follows `--last` and falls back to
  `auto` in a folder with no session.
- Claude accounts share `~/.claude` (0071), so any account can resume any Claude chat.
- An `auto` pick first refreshes the provider's usage when its cache file is missing or older than
  5 minutes, the freshness limit of the usage meters. The file age decides, so an account whose read
  keeps failing cannot force a refresh on every start. `swarm run` and `swarm launch` use the same
  rule. A refresh costs about 4 s on a Mac with four Codex homes, at most once in 5 minutes.
- The account reads run at the same time, one per home, so a named pick of the last home finishes
  inside the 2 s pick limit. In a row, a Mac with four Codex homes ran out of time at the third.

## Consequences

- Good: Pi, `claude`, `codex`, `swarm run` and `swarm launch` pick by the same rule, so the shell
  no longer loads yelo.
- Bad: a valueless `--profile` no longer opens yelo's numbered menu, so the owner must type a name.
- Bad: `codex profile list|create|sessions` and `claude profile list|create` are gone, and
  `swarm accounts --provider <claude|codex> --json` lists the accounts.
- Bad: `claude` and Pi's Claude bridge now use the native Claude home with the account's Keychain
  entry (0071), so chats that yelo kept in `~/.claude/.profiles/NAME` are not in their history.

## Links

- Related: 0063, 0071, 0074.
