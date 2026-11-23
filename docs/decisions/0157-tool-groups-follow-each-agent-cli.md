---
status: accepted
date: 2026-11-23
deciders: [user]
amends: ["0047"]
related: ["0134", "0139"]
informed-by:
  - 'owner, 2026-11-23: a pi chat showed each `bash` call and the thinking after it as a "1 command" group, with a second header, check mark, and time above the card'
  - 'owner, 2026-11-23: "if pi groups it then we group it": groups map one to one to what each agent CLI shows; scope "All providers"'
  - "Research 2026-11-23 of the shipped CLIs: pi 1.5.2, Claude Code 2.1.329, Codex 0.169.0, AGY"
---

# 0157. Tool groups follow each agent CLI

## Context and Problem Statement

ADR 0047 folds every run of 2 or more steps between prose, for all providers. A step was a tool
row, a thought, a ring, or a hidden row. A pi chat therefore showed `bash` plus its next thought as
"1 command", a group that pi never draws. The owner wants the chat to group tool calls only where
the agent CLI itself groups them.

## Decision Outcome

- Each provider's normalizer marks a tool call with a group key. Only consecutive calls with the
  same key fold, at most `FOLD_STEP_LIMIT` steps. A thought or a hidden row inside an open group
  joins it; any other row ends it. The `fold` wire shape stays the same.
- **Claude Code** marks `read_search` calls, as its `collapsed_read_search` row does: Read, Grep,
  Glob, and a Bash command whose every segment is a read, search, or list word with no redirect
  except to `/dev/null`. One call is enough. The label uses its words, for example "Searched for
  2 patterns, read 3 files, listed 1 directory", and "Searching for …, reading …" while it runs.
  MCP calls stay alone until their label is known.
- **Codex** marks `explore` calls, as its "Explored" cell does: every `parsed_cmd` part is `read`,
  `list_files`, or `search`. The mark comes from `exec_command_end` or `item_completed`, joined to
  the call by its id. The label is "Exploring" while it runs, then "Explored".
- **pi** marks nothing, because pi draws each call and each thought alone.
- **AGY** marks nothing until its grouping rule is confirmed.
- The folding rules of ADR 0047 for open state (seen rows, failed steps, the owner's choice, Chat
  detail) stay.

## Consequences

- Good: a chat looks like the CLI that wrote it; thinking stays a top-level row in pi.
- Good: each rule lives in the provider file that reads that log format.
- Bad: a long run of mixed tools in pi or AGY no longer folds into one line.
- Neutral: a new CLI version that changes its grouping needs a matching normalizer change.
