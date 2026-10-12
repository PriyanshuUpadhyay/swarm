---
status: proposed
date: 2026-10-12
deciders: [user]
related: ["0027", "0042", "0063", "0065"]
informed-by:
  - 'owner, 2026-10-12: "swarm should hold everything and not yelo"'
  - 'owner, 2026-10-12: "nothing in the user''s home is impacted and we keep it in our own isolate"'
  - 'owner, 2026-10-12: "keeping the chats where they are and just make it use the keychain of whichever profile is chosen"'
  - "~/dotfiles/docs/council/2026-10-12-swarm-owns-accounts-isolation.md"
  - "~/.claude/reports/2026-10-12-claude-login-only-switch.md"
---

# 0071. A Claude account is a Keychain entry, and chats use the native home

## Context and Problem Statement

ADR 0063 made a Claude account a profile folder in `~/.claude/.profiles/NAME`, passed as
`CLAUDE_CONFIG_DIR`. That folder shares its content with `~/.claude` through links, so swarm had to
create, link and repair folders in the owner's home. The owner wants no new swarm write there, and
wants one set of chats, settings and memory for every account.

## Decision Drivers

- No new swarm write in the owner's home.
- One chat history for all accounts.
- Fewer concepts: no mirror, no sync, no drift state.

## Considered Options

- A Keychain entry per account, and no config folder for chats.
- A profile folder per account under the swarm home, with links into `~/.claude` (council verdict of 2026-10-12, before the test).
- A private copy of the provider state under the swarm home.

## Decision Outcome

Chosen: a Keychain entry per account. Claude Code 2.1.296 names its Keychain entry from
`CLAUDE_SECURESTORAGE_CONFIG_DIR` alone when that variable is set. A test on 2026-10-12 ran two
accounts in one shared config folder, also at the same time, and an account with no usage left
failed on its own limit. So the entry decides who pays.

- A chat launch sets `CLAUDE_SECURESTORAGE_CONFIG_DIR` and `AGENT_PROFILE_LABEL`, and never
  `CLAUDE_CONFIG_DIR`. Chats, settings and memory stay in `~/.claude`.
- Sign-in and the sign-in check run with `CLAUDE_CONFIG_DIR` set to the account's own folder. For a
  new account that folder is `root_dir/accounts/claude/NAME`, mode 0700, and its path is also the
  secure-storage string. So a login never rewrites the account record in `~/.claude.json`, and two
  builds never share a Keychain entry.
- A yelo profile in `~/.claude/.profiles/NAME` is used by reference and keeps the string
  `$HOME/.claude-NAME`. Existing Codex homes and both default accounts stay by reference. A new
  Codex home is `root_dir/accounts/codex/NAME`.
- Reset and every cleanup skip `accounts/`.
- The run script starts the CLI with `command <cli>`, so a shell function cannot change the account.

Rejected: the linked folder, because it keeps a mirror to repair and writes links for no gain once
the config folder is not needed. Rejected: the private copy, because a plain `claude` and a swarm
chat would then hold different chats and settings.

This record changes the account-home parts of ADR 0063 and the "no secret under the swarm home"
consequence of ADR 0065. Their other rules stand.

## Consequences

- Good: no folder to create, link or repair in the owner's home; one chat history.
- Bad: the variable is not documented by the vendor and can change. The sign-in check then reports
  `signed_out`.
- Bad: the account record and some org caches in `~/.claude.json` are shared by all accounts.
  Accounts on different plans can read the wrong cached model access.
- Bad: login state for new accounts now sits under the swarm home, so deleting that home signs
  those accounts out.
