---
status: accepted
date: 2026-10-03
deciders: [user]
related: ["0028", "0038"]
informed-by:
  - "User direction on 2026-10-03 (flow chat-ui): good UI for the chat; no raw XML from shell mode or skills in You bubbles"
  - "Claude Code 2.1.263-2.1.288 logs: a ! command is a <bash-input> record and an output record whose parentUuid is the input; a skill body is an isMeta record linked by parentUuid or sourceToolUseID"
  - "docs/research/2026-10-03-claude-injected-records.md, docs/council/2026-10-03-chat-ui-direction.md (unanimous GO-WITH-CHANGES)"
---

# 0039. Claude's injected records become typed rows

## Context and Problem Statement

Claude Code writes shell-mode commands, slash-command echoes, skill bodies, reminders, and other
notices as user records, but the owner did not type them. It also writes some slash-command echoes
and output as `system` records with subtype `local_command`. Swarm showed them as raw XML in "You"
rows, so a "You" bubble held XML that the owner never wrote.

## Considered Options

- Hide every injected record by its text prefix, as ruddr does.
- Tag each record in the parser and pair it with its parent by id in the row builder.

## Decision Outcome

Chosen: tag and pair. The Zig parser gives each injected record its own kind (`shell_input`,
`shell_output`, `skill_body`, `interrupted`, `injected`, `peer_message`) and passes `parent_uuid`
and `source_tool_use_id` on every event. A `local_command` record whose text is a command echo or
output takes the `command` or `command_output` kind, as a user record does. The Swift row builder links a record only when exactly one
parent matches in the same scope, so a `!` pair becomes one shell row, a skill body folds under its
command chip or Skill tool row, and an unmatched record stays visible. Unknown injected text becomes
a hidden System row, never a "You" row. Rejected: prefix hiding, because it loses the shell output
and the skill name, and a typed prompt that starts with `<` would vanish.

### Consequences

- Good: a `!` command shows as one shell row and still starts a turn, because Claude Code answers
  it (6 of 6 local shell outputs were followed by an assistant reply); an interrupt ends a turn.
- Good: a message from another agent (`peer_message`) is a visible notice that starts a turn.
- Good: a run of three or more finished tools folds after its turn ends; a row on screen never folds
  by itself (ADR 0028).
- Bad: every event carries two more `meta` fields, about 40 bytes each.
- Neutral: Textual stays out; a markdown package needs its own spike and ADR.
